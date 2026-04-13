pub const SSAO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

pub struct SsaoRenderer {
    pipeline: wgpu::RenderPipeline,
    depth_bind_group: wgpu::BindGroup,
    ao_view: wgpu::TextureView,
}

impl SsaoRenderer {
    /// Creates SSAO renderer. AO texture is half-resolution for natural smoothing.
    pub fn new(
        device: &wgpu::Device,
        uniform_bgl: &wgpu::BindGroupLayout,
        depth_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSAO Shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}\n{}\n{}\n{}\n{}", include_str!("uniforms.wgsl"), include_str!("noise.wgsl"), include_str!("depth_utils.wgsl"), include_str!("fullscreen.wgsl"), include_str!("ssao.wgsl")).into(),
            ),
        });

        let depth_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO Depth BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSAO Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl, &depth_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = crate::pipeline::create_fullscreen_pipeline(
            device, "SSAO Pipeline", &shader, &pipeline_layout, SSAO_FORMAT,
        );

        let depth_bind_group = Self::create_depth_bg(device, &depth_bgl, depth_view);
        let ao_view = Self::create_ao_texture(device, (width / 2).max(1), (height / 2).max(1));

        Self {
            pipeline,
            depth_bind_group,
            ao_view,
        }
    }

    fn create_depth_bg(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        depth_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO Depth BG"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(depth_view),
            }],
        })
    }

    fn create_ao_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSAO AO"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SSAO_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn ao_view(&self) -> &wgpu::TextureView {
        &self.ao_view
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bind_group: &'a wgpu::BindGroup,
    ) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, uniform_bind_group, &[]);
        pass.set_bind_group(1, &self.depth_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
