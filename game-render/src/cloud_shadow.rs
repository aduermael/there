pub const CLOUD_SHADOW_SIZE: u32 = 256;

pub struct CloudShadowRenderer {
    pipeline: wgpu::ComputePipeline,
    uniform_bg: wgpu::BindGroup,
    output_bg: wgpu::BindGroup,
    texture_view: wgpu::TextureView,
}

impl CloudShadowRenderer {
    pub fn new(
        device: &wgpu::Device,
        uniform_bgl: &wgpu::BindGroupLayout,
        uniform_buffer: &wgpu::Buffer,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cloud Shadow Compute"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}\n{}",
                    include_str!("uniforms.wgsl"),
                    include_str!("noise.wgsl"),
                    include_str!("cloud_shadow_compute.wgsl"),
                )
                .into(),
            ),
        });

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Cloud Shadow Texture"),
            size: wgpu::Extent3d {
                width: CLOUD_SHADOW_SIZE,
                height: CLOUD_SHADOW_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let output_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Cloud Shadow Output BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            }],
        });

        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cloud Shadow BG0 (Uniforms)"),
            layout: uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let output_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cloud Shadow BG1 (Output)"),
            layout: &output_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Cloud Shadow Compute Layout"),
            bind_group_layouts: &[uniform_bgl, &output_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Cloud Shadow Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            pipeline,
            uniform_bg,
            output_bg,
            texture_view,
        }
    }

    /// Dispatch the compute shader to bake cloud shadows for the current frame.
    pub fn compute(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Cloud Shadow Compute"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bg, &[]);
        pass.set_bind_group(1, &self.output_bg, &[]);
        pass.dispatch_workgroups(CLOUD_SHADOW_SIZE / 16, CLOUD_SHADOW_SIZE / 16, 1);
    }

    /// Texture view for binding in scene shaders (sampling the baked cloud shadow).
    pub fn shadow_view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }
}
