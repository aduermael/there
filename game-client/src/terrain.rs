use wgpu::util::DeviceExt;

const GRID_SIZE: u32 = 256;
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
    pub _pad2: [f32; 2],
}

pub struct TerrainRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    heightmap_bind_group: wgpu::BindGroup,
}

impl TerrainRenderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        heightmap_view: &wgpu::TextureView,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terrain.wgsl").into()),
        });

        // Generate flat grid mesh
        let (vertices, indices) = generate_grid(GRID_SIZE, game_core::WORLD_SIZE);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Terrain Indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let index_count = indices.len() as u32;

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

        // Pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terrain Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl, &heightmap_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terrain Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 8, // 2 x f32
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

        log::info!(
            "Terrain: {}x{} grid, {} vertices, {} triangles",
            GRID_SIZE,
            GRID_SIZE,
            (GRID_SIZE + 1) * (GRID_SIZE + 1),
            index_count / 3
        );

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count,
            heightmap_bind_group,
        }
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, uniform_bg: &'a wgpu::BindGroup) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, uniform_bg, &[]);
        pass.set_bind_group(1, &self.heightmap_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
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

/// Generate a flat XZ grid mesh. Each vertex is [x, z].
fn generate_grid(grid_size: u32, world_size: f32) -> (Vec<[f32; 2]>, Vec<u32>) {
    let verts_per_side = grid_size + 1;
    let step = world_size / grid_size as f32;

    let mut vertices = Vec::with_capacity((verts_per_side * verts_per_side) as usize);
    for iz in 0..verts_per_side {
        for ix in 0..verts_per_side {
            vertices.push([ix as f32 * step, iz as f32 * step]);
        }
    }

    let mut indices = Vec::with_capacity((grid_size * grid_size * 6) as usize);
    for iz in 0..grid_size {
        for ix in 0..grid_size {
            let tl = iz * verts_per_side + ix;
            let tr = tl + 1;
            let bl = (iz + 1) * verts_per_side + ix;
            let br = bl + 1;

            // Two CCW triangles per quad
            indices.push(tl);
            indices.push(bl);
            indices.push(tr);

            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }

    (vertices, indices)
}
