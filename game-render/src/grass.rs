use wgpu::util::DeviceExt;

use crate::DEPTH_FORMAT;

pub const MAX_GRASS: usize = 24000;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GrassInstance {
    pub pos_scale: [f32; 4],      // x, y, z, height_scale
    pub color_rotation: [f32; 4], // r, g, b, y_rotation
}

pub struct GrassRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    instance_count: u32,
}

impl GrassRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        shadow_bgl: &wgpu::BindGroupLayout,
        instances: &[GrassInstance],
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grass Shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}\n{}", include_str!("common.wgsl"), include_str!("grass.wgsl")).into(),
            ),
        });

        let (vertices, indices) = generate_grass_blade();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grass Verts"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_count = indices.len() as u32;
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grass Idx"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grass Instances"),
            size: (MAX_GRASS * std::mem::size_of::<GrassInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let instance_count = instances.len().min(MAX_GRASS) as u32;
        if !instances.is_empty() {
            queue.write_buffer(
                &instance_buffer,
                0,
                bytemuck::cast_slice(&instances[..instance_count as usize]),
            );
        }

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Grass Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl, shadow_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grass Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // Per-vertex: position (vec3) + bend (f32) = 16 bytes
                    wgpu::VertexBufferLayout {
                        array_stride: 16,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32,
                                offset: 12,
                                shader_location: 1,
                            },
                        ],
                    },
                    // Per-instance: pos_scale + color_rotation = 32 bytes
                    wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 0,
                                shader_location: 2,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 16,
                                shader_location: 3,
                            },
                        ],
                    },
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // grass blades are thin, visible from both sides
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        log::info!("Grass renderer: {} instances", instance_count);

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count,
            instance_buffer,
            instance_count,
        }
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bg: &'a wgpu::BindGroup,
        shadow_bg: &'a wgpu::BindGroup,
    ) {
        if self.instance_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, uniform_bg, &[]);
        pass.set_bind_group(1, shadow_bg, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..self.instance_count);
    }
}

/// Vertex with position and bend factor (0 at base, 1 at tip).
type GrassVertex = [f32; 4]; // [x, y, z, bend]

/// Generate a curved grass blade: 3 segments (base → mid → tip), 6 vertices, 4 triangles.
/// Wider at base for a tuft-like silhouette, slight mid-height lean for natural curve.
fn generate_grass_blade() -> (Vec<GrassVertex>, Vec<u32>) {
    let base_hw = 0.14;  // half-width at base (~0.28 total)
    let mid_hw = 0.10;   // half-width at mid (~0.20 total)
    let tip_hw = 0.03;   // half-width at tip (~0.06 total)
    let mid_h = 0.25;
    let tip_h = 0.60;
    let lean = 0.03;     // slight X offset at mid for natural curve

    let verts = vec![
        [-base_hw, 0.0,   0.0, 0.0],  // 0: base-left  (anchored)
        [ base_hw, 0.0,   0.0, 0.0],  // 1: base-right (anchored)
        [-mid_hw + lean, mid_h, 0.0, 0.4],  // 2: mid-left  (partial bend)
        [ mid_hw + lean, mid_h, 0.0, 0.4],  // 3: mid-right (partial bend)
        [-tip_hw + lean, tip_h, 0.0, 1.0],  // 4: tip-left  (full bend)
        [ tip_hw + lean, tip_h, 0.0, 1.0],  // 5: tip-right (full bend)
    ];

    let indices = vec![
        0, 1, 2,  1, 3, 2,  // base quad
        2, 3, 4,  3, 5, 4,  // tip quad
    ];

    (verts, indices)
}
