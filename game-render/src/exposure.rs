use wgpu::util::DeviceExt;

const HISTOGRAM_BINS: u32 = 256;
const WG_SIZE: u32 = 16;

pub struct ExposureRenderer {
    histogram_pipeline: wgpu::ComputePipeline,
    reduce_pipeline: wgpu::ComputePipeline,
    histogram_buffer: wgpu::Buffer,
    exposure_buffer: wgpu::Buffer,
    histogram_bgl: wgpu::BindGroupLayout,
    histogram_bg: wgpu::BindGroup,
    _reduce_bgl: wgpu::BindGroupLayout,
    reduce_bg: wgpu::BindGroup,
    width: u32,
    height: u32,
}

impl ExposureRenderer {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let histogram_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Exposure Histogram Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("exposure_histogram.wgsl").into(),
            ),
        });

        let reduce_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Exposure Reduce Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("exposure_reduce.wgsl").into(),
            ),
        });

        // Histogram buffer (256 u32s, cleared each frame)
        let histogram_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Exposure Histogram"),
            size: (HISTOGRAM_BINS * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Exposure buffer (single f32, initialized to 1.0)
        let exposure_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Exposure Value"),
            contents: bytemuck::bytes_of(&1.0f32),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Histogram pass BGL: group 0 = {HDR texture, histogram buffer (rw atomic)}
        let histogram_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Exposure Histogram BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Reduce pass BGL: group 0 = {histogram buffer (read), exposure buffer (rw)}
        let reduce_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Exposure Reduce BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Reduce bind group (static - histogram + exposure buffers don't change)
        let reduce_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Exposure Reduce BG"),
            layout: &reduce_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: histogram_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: exposure_buffer.as_entire_binding(),
                },
            ],
        });

        // Pipelines
        let histogram_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Exposure Histogram Layout"),
            bind_group_layouts: &[&histogram_bgl],
            push_constant_ranges: &[],
        });

        let histogram_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Exposure Histogram"),
                layout: Some(&histogram_layout),
                module: &histogram_shader,
                entry_point: Some("cs_main"),
                compilation_options: Default::default(),
                cache: None,
            });

        let reduce_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Exposure Reduce Layout"),
            bind_group_layouts: &[&reduce_bgl],
            push_constant_ranges: &[],
        });

        let reduce_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Exposure Reduce"),
                layout: Some(&reduce_layout),
                module: &reduce_shader,
                entry_point: Some("cs_main"),
                compilation_options: Default::default(),
                cache: None,
            });

        // Histogram bind group starts empty (needs HDR view from build_bind_groups)
        let histogram_bg = Self::create_histogram_bg(device, &histogram_bgl, &histogram_buffer, width, height);

        log::info!("Exposure renderer: {} bins, {}x{}", HISTOGRAM_BINS, width, height);

        Self {
            histogram_pipeline,
            reduce_pipeline,
            histogram_buffer,
            exposure_buffer,
            histogram_bgl,
            histogram_bg,
            _reduce_bgl: reduce_bgl,
            reduce_bg,
            width,
            height,
        }
    }

    fn create_histogram_bg(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        histogram_buffer: &wgpu::Buffer,
        _width: u32,
        _height: u32,
    ) -> wgpu::BindGroup {
        // Placeholder - will be rebuilt with actual HDR view in build_bind_groups
        // Create a tiny placeholder texture
        let placeholder = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Exposure Placeholder"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = placeholder.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Exposure Histogram BG (placeholder)"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: histogram_buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// Rebuild bind groups with the actual HDR intermediate view.
    pub fn build_bind_groups(&mut self, device: &wgpu::Device, hdr_view: &wgpu::TextureView) {
        self.histogram_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Exposure Histogram BG"),
            layout: &self.histogram_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.histogram_buffer.as_entire_binding(),
                },
            ],
        });
    }

    pub fn resize(&mut self, _device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        // Bind groups rebuilt via build_bind_groups() when HDR view changes
    }

    /// Dispatch histogram + reduce compute passes.
    pub fn compute(&self, encoder: &mut wgpu::CommandEncoder) {
        // Clear histogram buffer
        encoder.clear_buffer(&self.histogram_buffer, 0, None);

        // Pass 1: Build histogram from HDR buffer
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Exposure Histogram"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.histogram_pipeline);
            pass.set_bind_group(0, &self.histogram_bg, &[]);
            pass.dispatch_workgroups(
                div_ceil(self.width, WG_SIZE),
                div_ceil(self.height, WG_SIZE),
                1,
            );
        }

        // Pass 2: Reduce histogram to exposure value
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Exposure Reduce"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.reduce_pipeline);
            pass.set_bind_group(0, &self.reduce_bg, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
    }

    /// Get the exposure buffer for binding in postprocess.
    pub fn exposure_buffer(&self) -> &wgpu::Buffer {
        &self.exposure_buffer
    }
}

fn div_ceil(a: u32, b: u32) -> u32 {
    (a + b - 1) / b
}
