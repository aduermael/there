use wgpu::util::DeviceExt;



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
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
}

impl PlayerRenderer {
    pub fn new(
        device: &wgpu::Device,
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

        let (vertices, indices) = generate_capsule(
            CAPSULE_RADIUS,
            CAPSULE_CYL_HEIGHT,
            CAPSULE_SEGMENTS,
            CAPSULE_RINGS,
        );

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Capsule Verts"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_count = indices.len() as u32;
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Capsule Idx"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Player Instances"),
            size: (MAX_PLAYERS * std::mem::size_of::<PlayerInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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

        log::info!(
            "Player renderer: {} verts, {} tris, max {} instances",
            vertices.len(),
            index_count / 3,
            MAX_PLAYERS,
        );

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count,
            instance_buffer,
        }
    }

    pub fn update_instances(&self, queue: &wgpu::Queue, instances: &[PlayerInstance]) {
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bg: &'a wgpu::BindGroup,
        shadow_bg: &'a wgpu::BindGroup,
        instance_count: u32,
    ) {
        if instance_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, uniform_bg, &[]);
        pass.set_bind_group(1, shadow_bg, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..instance_count);
    }
}

/// Generate a capsule mesh with bottom at y=0.
/// Total height = cyl_height + 2 * radius.
fn generate_capsule(
    radius: f32,
    cyl_height: f32,
    segments: u32,
    rings: u32,
) -> (Vec<[f32; 3]>, Vec<u32>) {
    let top_center = radius + cyl_height; // y center of top hemisphere
    let bot_center = radius; // y center of bottom hemisphere
    let mut verts = Vec::new();
    let mut indices = Vec::new();

    // Top hemisphere: pole (i=0) down to equator (i=rings)
    for i in 0..=rings {
        let phi = (i as f32 / rings as f32) * std::f32::consts::FRAC_PI_2;
        let y = top_center + radius * phi.cos();
        let r = radius * phi.sin();
        for j in 0..=segments {
            let theta = (j as f32 / segments as f32) * std::f32::consts::TAU;
            verts.push([r * theta.cos(), y, r * theta.sin()]);
        }
    }

    // Bottom hemisphere: equator (i=0) down to pole (i=rings)
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

    // Top hemisphere triangles
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

    // Cylinder body: top equator → bottom equator
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

    // Bottom hemisphere triangles
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
