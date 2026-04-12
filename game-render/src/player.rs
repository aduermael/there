use crate::instanced_mesh::InstancedMeshRenderer;
use crate::skeleton::{self, NUM_BONES, Skeleton};

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

/// Per-vertex data for the humanoid mesh.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct HumanoidVertex {
    position: [f32; 3],
    normal: [f32; 3],
    bone_index: u32,
    _pad: u32,
}

pub struct PlayerRenderer {
    mesh: InstancedMeshRenderer,
    skeleton: Skeleton,
    bone_buffer: wgpu::Buffer,
    bone_bind_group: wgpu::BindGroup,
}

impl PlayerRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        uniform_bgl: &wgpu::BindGroupLayout,
        shadow_bgl: &wgpu::BindGroupLayout,
    ) -> Self {
        let skeleton = Skeleton::new();

        // Bone matrix storage buffer: MAX_PLAYERS * NUM_BONES * 64 bytes (mat4)
        let bone_buf_size = (MAX_PLAYERS * NUM_BONES * 64) as u64;
        let bone_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Player Bone Matrices"),
            size: bone_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bone_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bone BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bone_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bone BG"),
            layout: &bone_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: bone_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Player Shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}\n{}\n{}",
                    include_str!("uniforms.wgsl"),
                    include_str!("noise.wgsl"),
                    include_str!("common.wgsl"),
                    include_str!("player.wgsl"),
                )
                .into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Player Pipeline Layout"),
            bind_group_layouts: &[uniform_bgl, shadow_bgl, &bone_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = crate::pipeline::create_scene_pipeline(
            device,
            "Player Pipeline",
            &shader,
            &pipeline_layout,
            &[
                // Vertex buffer: position(3f) + normal(3f) + bone_index(u32) + pad(u32) = 32 bytes
                wgpu::VertexBufferLayout {
                    array_stride: 32,
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
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 24,
                            shader_location: 2,
                        },
                    ],
                },
                // Instance buffer: pos_yaw(4f) + color(4f) = 32 bytes
                wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 3,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 4,
                        },
                    ],
                },
            ],
            surface_format,
            Some(wgpu::Face::Back),
            wgpu::CompareFunction::Less,
        );

        let (vertices, indices) = generate_humanoid(&skeleton);

        let mesh = InstancedMeshRenderer::new(
            device,
            queue,
            pipeline,
            None,
            bytemuck::cast_slice(&vertices),
            &indices,
            std::mem::size_of::<PlayerInstance>(),
            MAX_PLAYERS,
            &[],
            "Player",
        );

        log::info!(
            "Player renderer: {} verts, {} tris, {} bones, max {} instances",
            vertices.len(),
            indices.len() / 3,
            NUM_BONES,
            MAX_PLAYERS,
        );

        // Upload bind-pose matrices for all slots
        let bind_matrices = skeleton.bind_pose_matrices();
        let mat_floats = matrices_to_floats(&bind_matrices);
        let mat_bytes: &[u8] = bytemuck::cast_slice(&mat_floats);
        for i in 0..MAX_PLAYERS {
            let offset = (i * NUM_BONES * 64) as u64;
            queue.write_buffer(&bone_buffer, offset, mat_bytes);
        }

        Self {
            mesh,
            skeleton,
            bone_buffer,
            bone_bind_group,
        }
    }

    pub fn skeleton(&self) -> &Skeleton {
        &self.skeleton
    }

    /// Upload skin matrices for a specific player instance slot.
    pub fn upload_bones(&self, queue: &wgpu::Queue, instance_index: usize, matrices: &[glam::Mat4; NUM_BONES]) {
        let offset = (instance_index * NUM_BONES * 64) as u64;
        let floats = matrices_to_floats(matrices);
        let bytes: &[u8] = bytemuck::cast_slice(&floats);
        queue.write_buffer(&self.bone_buffer, offset, bytes);
    }

    pub fn update_instances(&self, queue: &wgpu::Queue, instances: &[PlayerInstance]) {
        self.mesh
            .update_instances(queue, bytemuck::cast_slice(instances), instances.len() as u32);
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_bg: &'a wgpu::BindGroup,
        shadow_bg: &'a wgpu::BindGroup,
        _instance_count: u32,
    ) {
        pass.set_bind_group(2, &self.bone_bind_group, &[]);
        self.mesh.draw(pass, uniform_bg, shadow_bg);
    }
}

// ── Humanoid mesh generation ──────────────────────────────────────────────

/// Convert Mat4 array to flat f32 array for GPU upload.
fn matrices_to_floats(matrices: &[glam::Mat4; NUM_BONES]) -> Vec<f32> {
    let mut out = Vec::with_capacity(NUM_BONES * 16);
    for m in matrices {
        out.extend_from_slice(&m.to_cols_array());
    }
    out
}

/// Generate the full humanoid mesh in bind pose (T-pose).
fn generate_humanoid(skeleton: &Skeleton) -> (Vec<HumanoidVertex>, Vec<u32>) {
    let mut verts = Vec::with_capacity(600);
    let mut indices = Vec::with_capacity(2400);

    // Head — ellipsoid at head bone pos
    let head_pos = skeleton.bone_world_pos(skeleton::HEAD);
    add_ellipsoid(&mut verts, &mut indices, head_pos, 0.10, 0.12, 0.10, 8, 6, skeleton::HEAD);

    // Neck — small cylinder
    let neck_pos = skeleton.bone_world_pos(skeleton::NECK);
    add_box(&mut verts, &mut indices, neck_pos, 0.05, 0.08, 0.05, skeleton::NECK);

    // Chest — box
    let chest_pos = skeleton.bone_world_pos(skeleton::CHEST);
    add_box(&mut verts, &mut indices, chest_pos, 0.18, 0.12, 0.10, skeleton::CHEST);

    // Spine — box (slightly narrower)
    let spine_pos = skeleton.bone_world_pos(skeleton::SPINE);
    add_box(&mut verts, &mut indices, spine_pos, 0.16, 0.10, 0.09, skeleton::SPINE);

    // Hips — box
    let hips_pos = skeleton.bone_world_pos(skeleton::HIPS);
    add_box(&mut verts, &mut indices, hips_pos, 0.17, 0.08, 0.09, skeleton::HIPS);

    // Arms (upper + lower) — cylinders
    for &(upper, lower) in &[
        (skeleton::UPPER_ARM_L, skeleton::LOWER_ARM_L),
        (skeleton::UPPER_ARM_R, skeleton::LOWER_ARM_R),
    ] {
        let upper_pos = skeleton.bone_world_pos(upper);
        add_cylinder(&mut verts, &mut indices, upper_pos, 0.04, 0.26, 6, upper);
        let lower_pos = skeleton.bone_world_pos(lower);
        add_cylinder(&mut verts, &mut indices, lower_pos, 0.035, 0.26, 6, lower);
    }

    // Legs (upper + lower) — cylinders
    for &(upper, lower, foot) in &[
        (skeleton::UPPER_LEG_L, skeleton::LOWER_LEG_L, skeleton::FOOT_L),
        (skeleton::UPPER_LEG_R, skeleton::LOWER_LEG_R, skeleton::FOOT_R),
    ] {
        let upper_pos = skeleton.bone_world_pos(upper);
        add_cylinder(&mut verts, &mut indices, upper_pos, 0.055, 0.43, 6, upper);
        let lower_pos = skeleton.bone_world_pos(lower);
        add_cylinder(&mut verts, &mut indices, lower_pos, 0.045, 0.43, 6, lower);

        // Foot — small box
        let foot_pos = skeleton.bone_world_pos(foot);
        add_box(
            &mut verts,
            &mut indices,
            glam::Vec3::new(foot_pos.x, foot_pos.y + 0.02, foot_pos.z + 0.03),
            0.05,
            0.03,
            0.10,
            foot,
        );
    }

    log::info!("Humanoid mesh: {} verts, {} indices", verts.len(), indices.len());

    (verts, indices)
}

/// Add an axis-aligned box centered at `center` with half-extents hx, hy, hz.
fn add_box(
    verts: &mut Vec<HumanoidVertex>,
    indices: &mut Vec<u32>,
    center: glam::Vec3,
    hx: f32,
    hy: f32,
    hz: f32,
    bone: usize,
) {
    let bone_index = bone as u32;

    // 6 faces × 4 verts = 24 verts (unique normals per face)
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        // +Y (top)
        ([0.0, 1.0, 0.0], [
            [center.x - hx, center.y + hy, center.z - hz],
            [center.x + hx, center.y + hy, center.z - hz],
            [center.x + hx, center.y + hy, center.z + hz],
            [center.x - hx, center.y + hy, center.z + hz],
        ]),
        // -Y (bottom)
        ([0.0, -1.0, 0.0], [
            [center.x - hx, center.y - hy, center.z + hz],
            [center.x + hx, center.y - hy, center.z + hz],
            [center.x + hx, center.y - hy, center.z - hz],
            [center.x - hx, center.y - hy, center.z - hz],
        ]),
        // +X (right)
        ([1.0, 0.0, 0.0], [
            [center.x + hx, center.y - hy, center.z - hz],
            [center.x + hx, center.y - hy, center.z + hz],
            [center.x + hx, center.y + hy, center.z + hz],
            [center.x + hx, center.y + hy, center.z - hz],
        ]),
        // -X (left)
        ([-1.0, 0.0, 0.0], [
            [center.x - hx, center.y - hy, center.z + hz],
            [center.x - hx, center.y - hy, center.z - hz],
            [center.x - hx, center.y + hy, center.z - hz],
            [center.x - hx, center.y + hy, center.z + hz],
        ]),
        // +Z (front) — v1/v3 swapped for consistent CCW winding
        ([0.0, 0.0, 1.0], [
            [center.x - hx, center.y - hy, center.z + hz],
            [center.x - hx, center.y + hy, center.z + hz],
            [center.x + hx, center.y + hy, center.z + hz],
            [center.x + hx, center.y - hy, center.z + hz],
        ]),
        // -Z (back) — v1/v3 swapped for consistent CCW winding
        ([0.0, 0.0, -1.0], [
            [center.x + hx, center.y - hy, center.z - hz],
            [center.x + hx, center.y + hy, center.z - hz],
            [center.x - hx, center.y + hy, center.z - hz],
            [center.x - hx, center.y - hy, center.z - hz],
        ]),
    ];

    for (normal, positions) in &faces {
        let i = verts.len() as u32;
        for pos in positions {
            verts.push(HumanoidVertex {
                position: *pos,
                normal: *normal,
                bone_index,
                _pad: 0,
            });
        }
        indices.extend_from_slice(&[i, i + 2, i + 1, i, i + 3, i + 2]);
    }
}

/// Add a cylinder along the -Y axis from `top_center` downward by `height`.
fn add_cylinder(
    verts: &mut Vec<HumanoidVertex>,
    indices: &mut Vec<u32>,
    top_center: glam::Vec3,
    radius: f32,
    height: f32,
    segments: u32,
    bone: usize,
) {
    let bone_index = bone as u32;
    let base = verts.len() as u32;
    let bot_y = top_center.y - height;

    // Side vertices: 2 rings
    for ring in 0..2u32 {
        let y = if ring == 0 { top_center.y } else { bot_y };
        for j in 0..segments {
            let theta = (j as f32 / segments as f32) * std::f32::consts::TAU;
            let nx = theta.cos();
            let nz = theta.sin();
            verts.push(HumanoidVertex {
                position: [top_center.x + radius * nx, y, top_center.z + radius * nz],
                normal: [nx, 0.0, nz],
                bone_index,
                _pad: 0,
            });
        }
    }

    // Side indices
    for j in 0..segments {
        let t0 = base + j;
        let t1 = base + (j + 1) % segments;
        let b0 = base + segments + j;
        let b1 = base + segments + (j + 1) % segments;
        indices.extend_from_slice(&[t0, t1, b0, t1, b1, b0]);
    }

    // Top cap
    let top_center_idx = verts.len() as u32;
    verts.push(HumanoidVertex {
        position: [top_center.x, top_center.y, top_center.z],
        normal: [0.0, 1.0, 0.0],
        bone_index,
        _pad: 0,
    });
    for j in 0..segments {
        let theta = (j as f32 / segments as f32) * std::f32::consts::TAU;
        verts.push(HumanoidVertex {
            position: [top_center.x + radius * theta.cos(), top_center.y, top_center.z + radius * theta.sin()],
            normal: [0.0, 1.0, 0.0],
            bone_index,
            _pad: 0,
        });
    }
    for j in 0..segments {
        indices.extend_from_slice(&[
            top_center_idx,
            top_center_idx + 1 + (j + 1) % segments,
            top_center_idx + 1 + j,
        ]);
    }

    // Bottom cap
    let bot_center_idx = verts.len() as u32;
    verts.push(HumanoidVertex {
        position: [top_center.x, bot_y, top_center.z],
        normal: [0.0, -1.0, 0.0],
        bone_index,
        _pad: 0,
    });
    for j in 0..segments {
        let theta = (j as f32 / segments as f32) * std::f32::consts::TAU;
        verts.push(HumanoidVertex {
            position: [top_center.x + radius * theta.cos(), bot_y, top_center.z + radius * theta.sin()],
            normal: [0.0, -1.0, 0.0],
            bone_index,
            _pad: 0,
        });
    }
    for j in 0..segments {
        indices.extend_from_slice(&[
            bot_center_idx,
            bot_center_idx + 1 + j,
            bot_center_idx + 1 + (j + 1) % segments,
        ]);
    }
}

/// Add an ellipsoid centered at `center` with radii rx, ry, rz.
fn add_ellipsoid(
    verts: &mut Vec<HumanoidVertex>,
    indices: &mut Vec<u32>,
    center: glam::Vec3,
    rx: f32,
    ry: f32,
    rz: f32,
    lon_segments: u32,
    lat_segments: u32,
    bone: usize,
) {
    let bone_index = bone as u32;
    let base = verts.len() as u32;

    for lat in 0..=lat_segments {
        let phi = (lat as f32 / lat_segments as f32) * std::f32::consts::PI;
        let sp = phi.sin();
        let cp = phi.cos();
        for lon in 0..=lon_segments {
            let theta = (lon as f32 / lon_segments as f32) * std::f32::consts::TAU;
            let st = theta.sin();
            let ct = theta.cos();

            // Normal on unit sphere, then scale position by radii
            let nx = ct * sp;
            let ny = cp;
            let nz = st * sp;

            verts.push(HumanoidVertex {
                position: [center.x + rx * nx, center.y + ry * ny, center.z + rz * nz],
                normal: [nx, ny, nz], // approximate for non-uniform scaling — good enough
                bone_index,
                _pad: 0,
            });
        }
    }

    let stride = lon_segments + 1;
    for lat in 0..lat_segments {
        for lon in 0..lon_segments {
            let a = base + lat * stride + lon;
            let b = a + stride;
            indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
}
