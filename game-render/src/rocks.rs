use crate::instanced_mesh::InstancedMeshRenderer;

const MAX_ROCKS: usize = 1024;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RockInstance {
    pub pos_scale: [f32; 4], // x, y, z, uniform_scale
    pub color: [f32; 4],     // r, g, b, _pad
}

pub struct RockRenderer {
    mesh: InstancedMeshRenderer,
}

impl RockRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        shadow_bgl: &wgpu::BindGroupLayout,
        instances: &[RockInstance],
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rock Shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}\n{}\n{}\n{}", include_str!("uniforms.wgsl"), include_str!("noise.wgsl"), include_str!("common.wgsl"), include_str!("rocks.wgsl")).into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rock Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl, shadow_bgl],
            push_constant_ranges: &[],
        });

        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rock Shadow Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl],
            push_constant_ranges: &[],
        });

        let rock_vertex_layouts = &[
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
        ];

        let pipeline = crate::pipeline::create_scene_pipeline(
            device, "Rock Pipeline", &shader, &pipeline_layout,
            rock_vertex_layouts, surface_format,
            Some(wgpu::Face::Back), wgpu::CompareFunction::Less,
        );

        let shadow_pipeline = crate::pipeline::create_shadow_pipeline(
            device, "Rock Shadow Pipeline", &shader, &shadow_pipeline_layout,
            rock_vertex_layouts,
        );

        let (vertices, indices) = generate_rock_mesh(1.0, 1, 42);

        let mesh = InstancedMeshRenderer::new(
            device, queue, pipeline, Some(shadow_pipeline),
            bytemuck::cast_slice(&vertices), &indices,
            std::mem::size_of::<RockInstance>(), MAX_ROCKS,
            bytemuck::cast_slice(instances), "Rock",
        );

        log::info!(
            "Rock renderer: {} verts, {} tris, {} instances",
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

/// Generate a deformed icosphere rock mesh.
fn generate_rock_mesh(radius: f32, subdivisions: u32, seed: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let (mut verts, indices) = icosphere(subdivisions);

    for (i, v) in verts.iter_mut().enumerate() {
        let n = glam::Vec3::from(*v).normalize();
        let hash = simple_hash(seed.wrapping_add(i as u32));
        let displacement = (hash as f32 / u32::MAX as f32) * 0.6 - 0.3;
        let r = radius * (1.0 + displacement);
        *v = (n * r).to_array();
    }

    (verts, indices)
}

fn icosphere(subdivisions: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let t = (1.0 + 5.0_f32.sqrt()) / 2.0;

    let mut verts: Vec<glam::Vec3> = vec![
        glam::Vec3::new(-1.0, t, 0.0).normalize(),
        glam::Vec3::new(1.0, t, 0.0).normalize(),
        glam::Vec3::new(-1.0, -t, 0.0).normalize(),
        glam::Vec3::new(1.0, -t, 0.0).normalize(),
        glam::Vec3::new(0.0, -1.0, t).normalize(),
        glam::Vec3::new(0.0, 1.0, t).normalize(),
        glam::Vec3::new(0.0, -1.0, -t).normalize(),
        glam::Vec3::new(0.0, 1.0, -t).normalize(),
        glam::Vec3::new(t, 0.0, -1.0).normalize(),
        glam::Vec3::new(t, 0.0, 1.0).normalize(),
        glam::Vec3::new(-t, 0.0, -1.0).normalize(),
        glam::Vec3::new(-t, 0.0, 1.0).normalize(),
    ];

    let mut indices: Vec<[u32; 3]> = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];

    use std::collections::HashMap;
    let mut midpoint_cache: HashMap<(u32, u32), u32> = HashMap::new();

    for _ in 0..subdivisions {
        let mut new_indices = Vec::with_capacity(indices.len() * 4);
        midpoint_cache.clear();

        for tri in &indices {
            let a = get_midpoint(&mut verts, &mut midpoint_cache, tri[0], tri[1]);
            let b = get_midpoint(&mut verts, &mut midpoint_cache, tri[1], tri[2]);
            let c = get_midpoint(&mut verts, &mut midpoint_cache, tri[2], tri[0]);

            new_indices.push([tri[0], a, c]);
            new_indices.push([tri[1], b, a]);
            new_indices.push([tri[2], c, b]);
            new_indices.push([a, b, c]);
        }

        indices = new_indices;
    }

    let out_verts: Vec<[f32; 3]> = verts.iter().map(|v| v.to_array()).collect();
    let out_indices: Vec<u32> = indices.iter().flat_map(|t| t.iter().copied()).collect();
    (out_verts, out_indices)
}

fn get_midpoint(
    verts: &mut Vec<glam::Vec3>,
    cache: &mut std::collections::HashMap<(u32, u32), u32>,
    a: u32,
    b: u32,
) -> u32 {
    let key = if a < b { (a, b) } else { (b, a) };
    if let Some(&idx) = cache.get(&key) {
        return idx;
    }
    let mid = ((verts[a as usize] + verts[b as usize]) * 0.5).normalize();
    let idx = verts.len() as u32;
    verts.push(mid);
    cache.insert(key, idx);
    idx
}

fn simple_hash(mut x: u32) -> u32 {
    x = x.wrapping_mul(0x9e3779b9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85ebca6b);
    x ^= x >> 13;
    x = x.wrapping_mul(0xc2b2ae35);
    x ^= x >> 16;
    x
}
