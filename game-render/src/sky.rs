pub struct SkyRenderer {
    pipeline: wgpu::RenderPipeline,
}

impl SkyRenderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        shadow_bgl: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sky Shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}\n{}\n{}\n{}", include_str!("uniforms.wgsl"), include_str!("noise.wgsl"), include_str!("common.wgsl"), include_str!("sky.wgsl")).into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sky Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl, shadow_bgl],
            push_constant_ranges: &[],
        });

        // Sky writes at max depth so terrain/objects always draw in front
        let pipeline = crate::pipeline::create_scene_pipeline(
            device, "Sky Pipeline", &shader, &pipeline_layout,
            &[], surface_format,
            None, wgpu::CompareFunction::LessEqual,
        );

        Self { pipeline }
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bg: &'a wgpu::BindGroup,
        shadow_bg: &'a wgpu::BindGroup,
    ) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, uniform_bg, &[]);
        pass.set_bind_group(1, shadow_bg, &[]);
        pass.draw(0..3, 0..1); // fullscreen triangle
    }
}
