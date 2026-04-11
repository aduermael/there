use crate::instanced_mesh::InstancedMeshRenderer;

const CAPSULE_RADIUS: f32 = 0.3;
const CAPSULE_CYL_HEIGHT: f32 = 1.2;
const CAPSULE_SEGMENTS: u32 = 12;
const CAPSULE_RINGS: u32 = 4;
const MAX_PLAYERS: usize = 64;

const PLAYER_COLORS: [[f32; 3]; 8] = [
    [0.90, 0.30, 0.25], // red
    [0.25, 0.60, 0.90], // blue
    [0.30, 0.85, 0.40], // green
    [0.95, 0.75, 0.20], // yellow
    [0.80, 0.40, 0.90], // purple
    [0.95, 0.55, 0.25], // orange
    [0.25, 0.85, 0.85], // cyan
    [0.90, 0.45, 0.70], // pink
];

pub fn player_color(id: u16) -> [f32; 3] {
    PLAYER_COLORS[id as usize % PLAYER_COLORS.len()]
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PlayerInstance {
    pub pos_yaw: [f32; 4], // x, y, z, yaw
    pub color: [f32; 4],   // r, g, b, _pad
}

pub struct PlayerRenderer {
    mesh: InstancedMeshRenderer,
}

impl PlayerRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        shadow_bgl: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Player Shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}\n{}\n{}\n{}", include_str!("uniforms.wgsl"), include_str!("noise.wgsl"), include_str!("common.wgsl"), include_str!("player.wgsl")).into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Player Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl, shadow_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = crate::pipeline::create_scene_pipeline(
            device, "Player Pipeline", &shader, &pipeline_layout,
            &[
                wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 }],
                },
                wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 2 },
                    ],
                },
            ],
            surface_format,
            Some(wgpu::Face::Back), wgpu::CompareFunction::Less,
        );

        let (vertices, indices) = generate_capsule(
            CAPSULE_RADIUS, CAPSULE_CYL_HEIGHT, CAPSULE_SEGMENTS, CAPSULE_RINGS,
        );

        let mesh = InstancedMeshRenderer::new(
            device, queue, pipeline, None,
            bytemuck::cast_slice(&vertices), &indices,
            std::mem::size_of::<PlayerInstance>(), MAX_PLAYERS,
            &[], "Player",
        );

        log::info!(
            "Player renderer: {} verts, {} tris, max {} instances",
            vertices.len(), indices.len() / 3, MAX_PLAYERS,
        );

        Self { mesh }
    }

    pub fn update_instances(&self, queue: &wgpu::Queue, instances: &[PlayerInstance]) {
        self.mesh.update_instances(queue, bytemuck::cast_slice(instances), instances.len() as u32);
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bg: &'a wgpu::BindGroup,
        shadow_bg: &'a wgpu::BindGroup,
        instance_count: u32,
    ) {
        // Player draw uses stored instance_count from update_instances
        // but the caller also passes count for backward compat — use mesh's stored count
        let _ = instance_count;
        self.mesh.draw(pass, uniform_bg, shadow_bg);
    }
}

/// Generate a capsule mesh with bottom at y=0.
fn generate_capsule(
    radius: f32,
    cyl_height: f32,
    segments: u32,
    rings: u32,
) -> (Vec<[f32; 3]>, Vec<u32>) {
    let top_center = radius + cyl_height;
    let bot_center = radius;
    let mut verts = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=rings {
        let phi = (i as f32 / rings as f32) * std::f32::consts::FRAC_PI_2;
        let y = top_center + radius * phi.cos();
        let r = radius * phi.sin();
        for j in 0..=segments {
            let theta = (j as f32 / segments as f32) * std::f32::consts::TAU;
            verts.push([r * theta.cos(), y, r * theta.sin()]);
        }
    }

    for i in 0..=rings {
        let phi =
            std::f32::consts::FRAC_PI_2 + (i as f32 / rings as f32) * std::f32::consts::FRAC_PI_2;
        let y = bot_center + radius * phi.cos();
        let r = radius * phi.sin();
        for j in 0..=segments {
            let theta = (j as f32 / segments as f32) * std::f32::consts::TAU;
            verts.push([r * theta.cos(), y, r * theta.sin()]);
        }
    }

    let stride = segments + 1;

    for i in 0..rings {
        for j in 0..segments {
            let r0 = i * stride + j;
            let r1 = (i + 1) * stride + j;
            indices.push(r0);
            indices.push(r0 + 1);
            indices.push(r1);
            indices.push(r1);
            indices.push(r0 + 1);
            indices.push(r1 + 1);
        }
    }

    let top_eq = rings * stride;
    let bot_eq = (rings + 1) * stride;
    for j in 0..segments {
        indices.push(top_eq + j);
        indices.push(top_eq + j + 1);
        indices.push(bot_eq + j);
        indices.push(bot_eq + j);
        indices.push(top_eq + j + 1);
        indices.push(bot_eq + j + 1);
    }

    for i in 0..rings {
        let base = rings + 1;
        for j in 0..segments {
            let r0 = (base + i) * stride + j;
            let r1 = (base + i + 1) * stride + j;
            indices.push(r0);
            indices.push(r0 + 1);
            indices.push(r1);
            indices.push(r1);
            indices.push(r0 + 1);
            indices.push(r1 + 1);
        }
    }

    (verts, indices)
}
