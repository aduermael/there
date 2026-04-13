pub const INTERMEDIATE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct PostProcessRenderer {
    pipeline: wgpu::RenderPipeline,
    intermediate_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

impl PostProcessRenderer {
    pub fn new(
        device: &wgpu::Device,
        output_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        ao_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        bloom_view: &wgpu::TextureView,
        exposure_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PostProcess Shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}\n{}\n{}\n{}\n{}", include_str!("uniforms.wgsl"), include_str!("noise.wgsl"), include_str!("depth_utils.wgsl"), include_str!("fullscreen.wgsl"), include_str!("postprocess.wgsl")).into(),
            ),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PostProcess BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("PostProcess Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, uniform_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = crate::pipeline::create_fullscreen_pipeline(
            device, "PostProcess Pipeline", &shader, &pipeline_layout, output_format,
        );

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("PostProcess Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let (intermediate_view, bind_group) =
            Self::create_intermediate(device, &bind_group_layout, &sampler, ao_view, depth_view, bloom_view, exposure_buffer, width, height);

        Self {
            pipeline,
            intermediate_view,
            bind_group,
        }
    }

    fn create_intermediate(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        ao_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        bloom_view: &wgpu::TextureView,
        exposure_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> (wgpu::TextureView, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HDR Intermediate"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: INTERMEDIATE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PostProcess BG"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(ao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(bloom_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: exposure_buffer.as_entire_binding(),
                },
            ],
        });
        (view, bind_group)
    }

    pub fn intermediate_view(&self) -> &wgpu::TextureView {
        &self.intermediate_view
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bind_group: &'a wgpu::BindGroup,
    ) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(1, uniform_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
