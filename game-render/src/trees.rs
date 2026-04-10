use wgpu::util::DeviceExt;

use crate::DEPTH_FORMAT;

const MAX_TREES: usize = 1536;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TreeInstance {
    pub pos_scale: [f32; 4],    // x, y, z, uniform_scale
    pub foliage_color: [f32; 4], // r, g, b, _pad
}

pub struct TreeRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    instance_count: u32,
}

impl TreeRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        instances: &[TreeInstance],
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tree Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("trees.wgsl").into()),
        });

        let (vertices, indices) = generate_tree_mesh(12);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tree Verts"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_count = indices.len() as u32;
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tree Idx"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Tree Instances"),
            size: (MAX_TREES * std::mem::size_of::<TreeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let instance_count = instances.len().min(MAX_TREES) as u32;
        if !instances.is_empty() {
            queue.write_buffer(
                &instance_buffer,
                0,
                bytemuck::cast_slice(&instances[..instance_count as usize]),
            );
        }

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tree Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Tree Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // Per-vertex: position (vec3) + color (vec3) = 24 bytes
                    wgpu::VertexBufferLayout {
                        array_stride: 24,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 12,
                                shader_location: 1,
                            },
                        ],
                    },
                    // Per-instance: pos_scale + foliage_color
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
                cull_mode: Some(wgpu::Face::Back),
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

        log::info!(
            "Tree renderer: {} verts, {} tris, {} instances",
            vertices.len(),
            index_count / 3,
            instance_count,
        );

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
    ) {
        if self.instance_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, uniform_bg, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..self.instance_count);
    }
}

/// Vertex with position and per-vertex color (trunk brown, foliage uses placeholder green
/// that gets multiplied by instance foliage_color in the shader).
type TreeVertex = [f32; 6]; // [x, y, z, r, g, b]

/// Generate combined trunk (cylinder) + foliage (cone) mesh.
/// `segments` controls circular resolution.
fn generate_tree_mesh(segments: u32) -> (Vec<TreeVertex>, Vec<u32>) {
    let trunk_radius = 0.15;
    let trunk_height = 1.0;
    let foliage_radius = 0.8;
    let foliage_height = 2.0;
    let trunk_color = [0.45, 0.30, 0.15];
    // Foliage base color — gets modulated by instance foliage_color
    let foliage_color = [1.0, 1.0, 1.0];

    let mut verts: Vec<TreeVertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // --- Trunk (open cylinder) ---
    let base_idx = verts.len() as u32;
    // Bottom ring
    for i in 0..=segments {
        let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let x = trunk_radius * theta.cos();
        let z = trunk_radius * theta.sin();
        verts.push([x, 0.0, z, trunk_color[0], trunk_color[1], trunk_color[2]]);
    }
    // Top ring
    for i in 0..=segments {
        let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let x = trunk_radius * theta.cos();
        let z = trunk_radius * theta.sin();
        verts.push([x, trunk_height, z, trunk_color[0], trunk_color[1], trunk_color[2]]);
    }
    let stride = segments + 1;
    for i in 0..segments {
        let b0 = base_idx + i;
        let b1 = base_idx + i + 1;
        let t0 = base_idx + stride + i;
        let t1 = base_idx + stride + i + 1;
        indices.push(b0);
        indices.push(t0);
        indices.push(b1);
        indices.push(b1);
        indices.push(t0);
        indices.push(t1);
    }

    // --- Foliage (cone) ---
    let cone_base_y = trunk_height * 0.6; // foliage starts partway up trunk
    let cone_tip_y = cone_base_y + foliage_height;

    // Tip vertex
    let tip_idx = verts.len() as u32;
    verts.push([0.0, cone_tip_y, 0.0, foliage_color[0], foliage_color[1], foliage_color[2]]);

    // Base ring
    let ring_start = verts.len() as u32;
    for i in 0..=segments {
        let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let x = foliage_radius * theta.cos();
        let z = foliage_radius * theta.sin();
        verts.push([x, cone_base_y, z, foliage_color[0], foliage_color[1], foliage_color[2]]);
    }

    // Cone sides
    for i in 0..segments {
        indices.push(tip_idx);
        indices.push(ring_start + i + 1);
        indices.push(ring_start + i);
    }

    // Cone bottom cap
    let center_idx = verts.len() as u32;
    verts.push([0.0, cone_base_y, 0.0, foliage_color[0], foliage_color[1], foliage_color[2]]);
    for i in 0..segments {
        indices.push(center_idx);
        indices.push(ring_start + i);
        indices.push(ring_start + i + 1);
    }

    (verts, indices)
}
