const BLOOM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const WG_SIZE: u32 = 8;

pub struct BloomRenderer {
    downsample_first_pipeline: wgpu::ComputePipeline,
    downsample_pipeline: wgpu::ComputePipeline,
    upsample_pipeline: wgpu::ComputePipeline,

    group0_bgl: wgpu::BindGroupLayout,
    group1_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,

    // Two textures with mip chains: downscale results + upscale accumulation
    _bloom_down: wgpu::Texture,
    _bloom_up: wgpu::Texture,
    down_views: Vec<wgpu::TextureView>,
    up_views: Vec<wgpu::TextureView>,

    // Pre-built bind groups (created in build_bind_groups)
    downsample_bgs: Vec<wgpu::BindGroup>,
    upsample_bgs: Vec<wgpu::BindGroup>,
    upsample_blend_bgs: Vec<wgpu::BindGroup>,

    mip_count: u32,
    mip_sizes: Vec<(u32, u32)>,
}

impl BloomRenderer {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, mip_count: u32) -> Self {
        let mip_sizes = compute_mip_sizes(width, height, mip_count);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bloom Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("bloom_compute.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Bloom Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        // Group 0: input texture + sampler + output storage texture
        let group0_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom Group0 BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: BLOOM_FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        // Group 1: blend texture for upsample (current level's downscale data)
        let group1_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom Group1 BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        // Downsample pipelines (group 0 only)
        let ds_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Bloom Downsample Layout"),
            bind_group_layouts: &[&group0_bgl],
            push_constant_ranges: &[],
        });

        let downsample_first_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Bloom Downsample First"),
                layout: Some(&ds_layout),
                module: &shader,
                entry_point: Some("cs_downsample_first"),
                compilation_options: Default::default(),
                cache: None,
            });

        let downsample_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Bloom Downsample"),
                layout: Some(&ds_layout),
                module: &shader,
                entry_point: Some("cs_downsample"),
                compilation_options: Default::default(),
                cache: None,
            });

        // Upsample pipeline (group 0 + group 1)
        let us_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Bloom Upsample Layout"),
            bind_group_layouts: &[&group0_bgl, &group1_bgl],
            push_constant_ranges: &[],
        });

        let upsample_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Bloom Upsample"),
                layout: Some(&us_layout),
                module: &shader,
                entry_point: Some("cs_upsample"),
                compilation_options: Default::default(),
                cache: None,
            });

        // Create bloom textures with mip chains
        let (_bloom_down, down_views) = create_bloom_texture(device, "Bloom Down", &mip_sizes);
        let (_bloom_up, up_views) = create_bloom_texture(device, "Bloom Up", &mip_sizes);

        log::info!(
            "Bloom renderer: {} mip levels, base {}x{}",
            mip_count,
            mip_sizes[0].0,
            mip_sizes[0].1,
        );

        Self {
            downsample_first_pipeline,
            downsample_pipeline,
            upsample_pipeline,
            group0_bgl,
            group1_bgl,
            sampler,
            _bloom_down,
            _bloom_up,
            down_views,
            up_views,
            downsample_bgs: Vec::new(),
            upsample_bgs: Vec::new(),
            upsample_blend_bgs: Vec::new(),
            mip_count,
            mip_sizes,
        }
    }

    /// Build all bind groups. Must be called after the HDR intermediate view is available.
    pub fn build_bind_groups(&mut self, device: &wgpu::Device, hdr_view: &wgpu::TextureView) {
        let mc = self.mip_count as usize;

        // Downsample bind groups (group 0): input → output for each mip level
        let mut downsample_bgs = Vec::with_capacity(mc);
        for i in 0..mc {
            let input_view = if i == 0 {
                hdr_view
            } else {
                &self.down_views[i - 1]
            };
            downsample_bgs.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Bloom DS BG{}", i)),
                layout: &self.group0_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(input_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&self.down_views[i]),
                    },
                ],
            }));
        }

        // Upsample bind groups: src (smaller upscale result) + blend (current downscale)
        let mut upsample_bgs = Vec::with_capacity(mc - 1);
        let mut upsample_blend_bgs = Vec::with_capacity(mc - 1);
        for i in 0..(mc - 1) {
            let out_mip = mc - 2 - i;
            let input_view = if i == 0 {
                &self.down_views[mc - 1] // deepest mip
            } else {
                &self.up_views[out_mip + 1] // previous upscale result
            };

            // Group 0: input (smaller) → output (current level)
            upsample_bgs.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Bloom US BG{}", i)),
                layout: &self.group0_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(input_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&self.up_views[out_mip]),
                    },
                ],
            }));

            // Group 1: blend with current level's downscale data
            upsample_blend_bgs.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Bloom US Blend BG{}", i)),
                layout: &self.group1_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.down_views[out_mip]),
                }],
            }));
        }

        self.downsample_bgs = downsample_bgs;
        self.upsample_bgs = upsample_bgs;
        self.upsample_blend_bgs = upsample_blend_bgs;
    }

    /// Dispatch all bloom compute passes (5 downscale + 4 upscale).
    /// Each pass is a separate compute pass for proper memory barriers.
    pub fn compute(&self, encoder: &mut wgpu::CommandEncoder) {
        let mc = self.mip_count as usize;

        // Downscale: HDR → mip 0 → mip 1 → ... → mip 4
        for i in 0..mc {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Bloom DS"),
                timestamp_writes: None,
            });
            if i == 0 {
                pass.set_pipeline(&self.downsample_first_pipeline);
            } else {
                pass.set_pipeline(&self.downsample_pipeline);
            }
            pass.set_bind_group(0, &self.downsample_bgs[i], &[]);
            let (w, h) = self.mip_sizes[i];
            pass.dispatch_workgroups(div_ceil(w, WG_SIZE), div_ceil(h, WG_SIZE), 1);
        }

        // Upscale: mip 4 → mip 3 → mip 2 → mip 1 → mip 0
        for i in 0..(mc - 1) {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Bloom US"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.upsample_pipeline);
            pass.set_bind_group(0, &self.upsample_bgs[i], &[]);
            pass.set_bind_group(1, &self.upsample_blend_bgs[i], &[]);
            let out_mip = mc - 2 - i;
            let (w, h) = self.mip_sizes[out_mip];
            pass.dispatch_workgroups(div_ceil(w, WG_SIZE), div_ceil(h, WG_SIZE), 1);
        }
    }

    /// View of the final bloom result (half-res, suitable for sampling in postprocess).
    pub fn result_view(&self) -> &wgpu::TextureView {
        &self.up_views[0]
    }

}

fn div_ceil(a: u32, b: u32) -> u32 {
    (a + b - 1) / b
}

fn compute_mip_sizes(width: u32, height: u32, mip_count: u32) -> Vec<(u32, u32)> {
    let mut sizes = Vec::with_capacity(mip_count as usize);
    let mut w = (width / 2).max(1);
    let mut h = (height / 2).max(1);
    for _ in 0..mip_count {
        sizes.push((w, h));
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    sizes
}

fn create_bloom_texture(
    device: &wgpu::Device,
    label: &str,
    mip_sizes: &[(u32, u32)],
) -> (wgpu::Texture, Vec<wgpu::TextureView>) {
    let (base_w, base_h) = mip_sizes[0];
    let mip_count = mip_sizes.len() as u32;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: base_w,
            height: base_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: BLOOM_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    });

    let views: Vec<wgpu::TextureView> = (0..mip_count)
        .map(|i| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("{} Mip{}", label, i)),
                base_mip_level: i,
                mip_level_count: Some(1),
                ..Default::default()
            })
        })
        .collect();

    (texture, views)
}
