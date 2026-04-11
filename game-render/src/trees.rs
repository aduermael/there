use crate::instanced_mesh::InstancedMeshRenderer;

pub const MAX_TREES: usize = 2048;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TreeInstance {
    pub pos_scale: [f32; 4],    // x, y, z, uniform_scale
    pub foliage_color: [f32; 4], // r, g, b, _pad
}

pub struct TreeRenderer {
    mesh: InstancedMeshRenderer,
}

impl TreeRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        shadow_bgl: &wgpu::BindGroupLayout,
        instances: &[TreeInstance],
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tree Shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}\n{}\n{}\n{}", include_str!("uniforms.wgsl"), include_str!("noise.wgsl"), include_str!("common.wgsl"), include_str!("trees.wgsl")).into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tree Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl, shadow_bgl],
            push_constant_ranges: &[],
        });

        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tree Shadow Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl],
            push_constant_ranges: &[],
        });

        let tree_vertex_layouts = &[
            wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 },
                ],
            },
            wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 2 },
                    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 3 },
                ],
            },
        ];

        let pipeline = crate::pipeline::create_scene_pipeline(
            device, "Tree Pipeline", &shader, &pipeline_layout,
            tree_vertex_layouts, surface_format,
            Some(wgpu::Face::Back), wgpu::CompareFunction::Less,
        );

        let shadow_pipeline = crate::pipeline::create_shadow_pipeline(
            device, "Tree Shadow Pipeline", &shader, &shadow_pipeline_layout,
            tree_vertex_layouts,
        );

        let (vertices, indices) = generate_tree_mesh(12);

        let mesh = InstancedMeshRenderer::new(
            device, queue, pipeline, Some(shadow_pipeline),
            bytemuck::cast_slice(&vertices), &indices,
            std::mem::size_of::<TreeInstance>(), MAX_TREES,
            bytemuck::cast_slice(instances), "Tree",
        );

        log::info!(
            "Tree renderer: {} verts, {} tris, {} instances",
            vertices.len(), indices.len() / 3, instances.len(),
        );

        Self { mesh }
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bg: &'a wgpu::BindGroup,
        shadow_bg: &'a wgpu::BindGroup,
    ) {
        self.mesh.draw(pass, uniform_bg, shadow_bg);
    }

    pub fn draw_shadow<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bg: &'a wgpu::BindGroup,
    ) {
        self.mesh.draw_shadow(pass, uniform_bg);
    }
}

/// Vertex with position and per-vertex color (trunk brown, foliage uses placeholder green
/// that gets multiplied by instance foliage_color in the shader).
type TreeVertex = [f32; 6]; // [x, y, z, r, g, b]

/// Generate combined trunk (cylinder) + multi-layered foliage (3 stacked cones) mesh.
/// `segments` controls circular resolution.
fn generate_tree_mesh(segments: u32) -> (Vec<TreeVertex>, Vec<u32>) {
    let trunk_radius = 0.15;
    let trunk_height = 1.0;
    let trunk_color = [0.45, 0.30, 0.15];

    let mut verts: Vec<TreeVertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // --- Trunk (open cylinder) ---
    let base_idx = verts.len() as u32;
    for i in 0..=segments {
        let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let x = trunk_radius * theta.cos();
        let z = trunk_radius * theta.sin();
        verts.push([x, 0.0, z, trunk_color[0], trunk_color[1], trunk_color[2]]);
    }
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

    // --- Foliage: 3 stacked cones (spruce/pine silhouette) ---
    let layers: [(f32, f32, f32, f32); 3] = [
        (0.5, 0.85, 1.3, 0.82),
        (1.1, 0.60, 1.1, 0.91),
        (1.6, 0.35, 0.9, 1.00),
    ];

    for (base_y, radius, height, shade) in layers {
        let tip_y = base_y + height;
        let foliage_color = [shade, shade, shade];

        let tip_idx = verts.len() as u32;
        verts.push([0.0, tip_y, 0.0, foliage_color[0], foliage_color[1], foliage_color[2]]);

        let ring_start = verts.len() as u32;
        for i in 0..=segments {
            let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let x = radius * theta.cos();
            let z = radius * theta.sin();
            verts.push([x, base_y, z, foliage_color[0], foliage_color[1], foliage_color[2]]);
        }

        for i in 0..segments {
            indices.push(tip_idx);
            indices.push(ring_start + i + 1);
            indices.push(ring_start + i);
        }

        let center_idx = verts.len() as u32;
        verts.push([0.0, base_y, 0.0, foliage_color[0], foliage_color[1], foliage_color[2]]);
        for i in 0..segments {
            indices.push(center_idx);
            indices.push(ring_start + i);
            indices.push(ring_start + i + 1);
        }
    }

    (verts, indices)
}
