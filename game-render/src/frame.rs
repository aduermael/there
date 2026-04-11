use crate::{
    BloomRenderer, GrassRenderer, PlayerRenderer, PostProcessRenderer, RockRenderer, SkyRenderer,
    SsaoRenderer, TerrainRenderer, TreeRenderer,
};

/// All the renderers needed to draw a complete frame.
pub struct SceneRenderers<'a> {
    pub terrain: &'a TerrainRenderer,
    pub sky: &'a SkyRenderer,
    pub grass: &'a GrassRenderer,
    pub rocks: &'a RockRenderer,
    pub trees: &'a TreeRenderer,
    pub players: Option<&'a PlayerRenderer>,
    pub ssao: &'a SsaoRenderer,
    pub bloom: &'a BloomRenderer,
    pub postprocess: &'a PostProcessRenderer,
}

/// Encode the full 5-pass frame pipeline into the given command encoder.
///
/// Pass sequence:
/// 0. Grass compute (instance generation)
/// 1. Shadow pass (depth from sun POV)
/// 2. Scene pass (HDR intermediate)
/// 3. SSAO pass (AO texture)
/// 4. Post-process pass (final output)
pub fn encode_frame(
    encoder: &mut wgpu::CommandEncoder,
    scene: &SceneRenderers,
    uniform_bg: &wgpu::BindGroup,
    shadow_bg: &wgpu::BindGroup,
    shadow_depth_view: &wgpu::TextureView,
    depth_view: &wgpu::TextureView,
    output_view: &wgpu::TextureView,
    camera_pos: glam::Vec3,
    view_proj: &glam::Mat4,
) {
    // Pass 0: Grass compute
    scene.grass.compute(encoder);

    // Pass 1: Shadow (depth from sun POV)
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow Pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: shadow_depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        scene.terrain.draw_shadow(&mut pass, uniform_bg);
        scene.rocks.draw_shadow(&mut pass, uniform_bg);
        scene.trees.draw_shadow(&mut pass, uniform_bg);
    }

    // Pass 2: Scene → HDR intermediate
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Scene Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene.postprocess.intermediate_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.53, g: 0.81, b: 0.92, a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        scene.sky.draw(&mut pass, uniform_bg, shadow_bg);
        scene.terrain.draw(&mut pass, uniform_bg, shadow_bg, camera_pos, view_proj);
        scene.grass.draw(&mut pass, uniform_bg, shadow_bg);
        scene.rocks.draw(&mut pass, uniform_bg, shadow_bg);
        scene.trees.draw(&mut pass, uniform_bg, shadow_bg);
        if let Some(players) = scene.players {
            players.draw(&mut pass, uniform_bg, shadow_bg, 0);
        }
    }

    // Pass 3: SSAO → AO texture
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SSAO Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene.ssao.ao_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        scene.ssao.draw(&mut pass, uniform_bg);
    }

    // Pass 3.5: Bloom compute (downscale + upscale mip chain)
    scene.bloom.compute(encoder);

    // Pass 4: Post-process → final output
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("PostProcess Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        scene.postprocess.draw(&mut pass, uniform_bg);
    }
}
