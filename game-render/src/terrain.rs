use wgpu::util::DeviceExt;

const CHUNK_COUNT: usize = 16;
const CHUNK_QUADS: u32 = 32;
const CHUNK_VERTS: u32 = CHUNK_QUADS + 1; // 33
const MIN_UNIFORM_ALIGN: u32 = 256;
const LOD_SWITCH_DISTANCE: f32 = 64.0; // switch to half-res beyond this
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub view_proj: [f32; 16],
    pub camera_pos: [f32; 3],
    pub _pad0: f32,
    pub sun_dir: [f32; 3],
    pub _pad1: f32,
    pub fog_color: [f32; 3],
    pub fog_far: f32,
    pub world_size: f32,
    pub hm_res: f32,
    pub ambient_intensity: f32,
    pub _pad2: f32,
    pub sun_color: [f32; 3],
    pub _pad3: f32,
    pub sky_zenith: [f32; 3],
    pub _pad4: f32,
    pub sky_horizon: [f32; 3],
    pub _pad5: f32,
}

pub struct TerrainRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    lod0_index_buffer: wgpu::Buffer,
    lod1_index_buffer: wgpu::Buffer,
    lod0_index_count: u32,
    lod1_index_count: u32,
    heightmap_bind_group: wgpu::BindGroup,
    chunk_bind_group: wgpu::BindGroup,
    chunk_bounds: Vec<(f32, f32)>, // per-chunk (min_y, max_y)
}

impl TerrainRenderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        heightmap_view: &wgpu::TextureView,
        heightmap_data: &[f32],
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terrain.wgsl").into()),
        });

        let chunk_size = game_core::WORLD_SIZE / CHUNK_COUNT as f32;

        // Shared vertex template: 33x33 grid for one chunk
        let vertices = generate_chunk_template(CHUNK_QUADS, chunk_size);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Chunk Verts"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // LOD 0: full-res (32x32 quads)
        let lod0_indices = generate_indices(CHUNK_QUADS, 1);
        let lod0_index_count = lod0_indices.len() as u32;
        let lod0_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain LOD0 Idx"),
            contents: bytemuck::cast_slice(&lod0_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // LOD 1: half-res (16x16 quads, skip every other vertex)
        let lod1_indices = generate_indices(CHUNK_QUADS, 2);
        let lod1_index_count = lod1_indices.len() as u32;
        let lod1_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain LOD1 Idx"),
            contents: bytemuck::cast_slice(&lod1_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Heightmap bind group (group 1)
        let heightmap_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Heightmap BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let heightmap_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Heightmap BG"),
            layout: &heightmap_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(heightmap_view),
            }],
        });

        // Per-chunk dynamic uniform buffer (group 2)
        let total_chunks = CHUNK_COUNT * CHUNK_COUNT;
        let mut chunk_data = vec![0u8; total_chunks * MIN_UNIFORM_ALIGN as usize];
        for cz in 0..CHUNK_COUNT {
            for cx in 0..CHUNK_COUNT {
                let byte_off = (cz * CHUNK_COUNT + cx) * MIN_UNIFORM_ALIGN as usize;
                let ox = cx as f32 * chunk_size;
                let oz = cz as f32 * chunk_size;
                chunk_data[byte_off..byte_off + 4].copy_from_slice(&ox.to_le_bytes());
                chunk_data[byte_off + 4..byte_off + 8].copy_from_slice(&oz.to_le_bytes());
            }
        }

        let chunk_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Chunk Offsets"),
            contents: &chunk_data,
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let chunk_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Chunk BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(8),
                },
                count: None,
            }],
        });

        let chunk_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Chunk BG"),
            layout: &chunk_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &chunk_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(8),
                }),
            }],
        });

        // Pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terrain Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl, &heightmap_bgl, &chunk_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terrain Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
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

        let chunk_bounds = compute_chunk_bounds(heightmap_data);

        log::info!(
            "Terrain: {}x{} chunks, {} verts/chunk, LOD0={} LOD1={} tris",
            CHUNK_COUNT,
            CHUNK_COUNT,
            CHUNK_VERTS * CHUNK_VERTS,
            lod0_index_count / 3,
            lod1_index_count / 3,
        );

        Self {
            pipeline,
            vertex_buffer,
            lod0_index_buffer,
            lod1_index_buffer,
            lod0_index_count,
            lod1_index_count,
            heightmap_bind_group,
            chunk_bind_group,
            chunk_bounds,
        }
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bg: &'a wgpu::BindGroup,
        camera_pos: glam::Vec3,
        vp: &glam::Mat4,
    ) {
        let frustum = extract_frustum_planes(vp);
        let chunk_size = game_core::WORLD_SIZE / CHUNK_COUNT as f32;

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, uniform_bg, &[]);
        pass.set_bind_group(1, &self.heightmap_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let mut drawn = 0u32;
        for cz in 0..CHUNK_COUNT {
            for cx in 0..CHUNK_COUNT {
                let idx = cz * CHUNK_COUNT + cx;
                let (min_y, max_y) = self.chunk_bounds[idx];
                let min = glam::Vec3::new(cx as f32 * chunk_size, min_y, cz as f32 * chunk_size);
                let max = glam::Vec3::new(
                    min.x + chunk_size,
                    max_y,
                    min.z + chunk_size,
                );

                if !aabb_in_frustum(min, max, &frustum) {
                    continue;
                }

                let center = (min + max) * 0.5;
                let dist = camera_pos.distance(center);
                let dyn_offset = (idx as u32) * MIN_UNIFORM_ALIGN;
                pass.set_bind_group(2, &self.chunk_bind_group, &[dyn_offset]);

                if dist > LOD_SWITCH_DISTANCE {
                    pass.set_index_buffer(
                        self.lod1_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    pass.draw_indexed(0..self.lod1_index_count, 0, 0..1);
                } else {
                    pass.set_index_buffer(
                        self.lod0_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    pass.draw_indexed(0..self.lod0_index_count, 0, 0..1);
                }
                drawn += 1;
            }
        }
        let _ = drawn; // will be used for debug stats later
    }
}

pub fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

// --- Mesh generation ---

fn generate_chunk_template(quads: u32, chunk_size: f32) -> Vec<[f32; 2]> {
    let vps = quads + 1;
    let step = chunk_size / quads as f32;
    let mut verts = Vec::with_capacity((vps * vps) as usize);
    for iz in 0..vps {
        for ix in 0..vps {
            verts.push([ix as f32 * step, iz as f32 * step]);
        }
    }
    verts
}

/// Generate index buffer for a chunk. `stride` = 1 for LOD0, 2 for LOD1.
fn generate_indices(quads: u32, stride: u32) -> Vec<u32> {
    let vps = quads + 1;
    let quad_count = quads / stride;
    let mut indices = Vec::with_capacity((quad_count * quad_count * 6) as usize);
    for qz in 0..quad_count {
        for qx in 0..quad_count {
            let tl = (qz * stride) * vps + (qx * stride);
            let tr = tl + stride;
            let bl = tl + stride * vps;
            let br = bl + stride;

            indices.push(tl);
            indices.push(bl);
            indices.push(tr);

            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }
    indices
}

// --- Frustum culling ---

fn extract_frustum_planes(vp: &glam::Mat4) -> [glam::Vec4; 6] {
    let r0 = vp.row(0);
    let r1 = vp.row(1);
    let r2 = vp.row(2);
    let r3 = vp.row(3);
    [
        r3 + r0, // left
        r3 - r0, // right
        r3 + r1, // bottom
        r3 - r1, // top
        r2,       // near (z >= 0)
        r3 - r2, // far
    ]
}

fn aabb_in_frustum(min: glam::Vec3, max: glam::Vec3, planes: &[glam::Vec4; 6]) -> bool {
    for p in planes {
        let n = glam::Vec3::new(p.x, p.y, p.z);
        // "Positive vertex" — corner most in the direction of the plane normal
        let pv = glam::Vec3::new(
            if n.x >= 0.0 { max.x } else { min.x },
            if n.y >= 0.0 { max.y } else { min.y },
            if n.z >= 0.0 { max.z } else { min.z },
        );
        if n.dot(pv) + p.w < 0.0 {
            return false;
        }
    }
    true
}

// --- Height bounds per chunk ---

fn compute_chunk_bounds(heightmap: &[f32]) -> Vec<(f32, f32)> {
    let hm = game_core::HEIGHTMAP_RES as usize;
    let texels_per_chunk = hm / CHUNK_COUNT;
    let mut bounds = Vec::with_capacity(CHUNK_COUNT * CHUNK_COUNT);

    for cz in 0..CHUNK_COUNT {
        for cx in 0..CHUNK_COUNT {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            let sx = cx * texels_per_chunk;
            let sz = cz * texels_per_chunk;
            // Include one extra texel to cover chunk edge
            for tz in sz..=(sz + texels_per_chunk).min(hm - 1) {
                for tx in sx..=(sx + texels_per_chunk).min(hm - 1) {
                    let h = heightmap[tz * hm + tx];
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            bounds.push((lo, hi));
        }
    }

    bounds
}
