use crate::DEPTH_FORMAT;

pub const SHADOW_MAP_SIZE: u32 = 1024;
pub const CASCADE_COUNT: usize = 3;
/// Near, mid, far cascade split distances (world units from camera).
pub const CASCADE_SPLITS: [f32; 3] = [20.0, 60.0, 200.0];

/// Byte offset of `sun_view_proj` within the Uniforms struct.
const SUN_VP_OFFSET: u64 = 272;
/// Size of one mat4x4<f32> in bytes.
const MAT4_SIZE: u64 = 64;

/// Shadow cascade texture + views + VP staging buffer.
pub struct ShadowCascades {
    pub _texture: wgpu::Texture,
    /// 2D-array view for scene sampling (all 3 layers).
    pub array_view: wgpu::TextureView,
    /// Per-layer views for rendering into each cascade.
    pub cascade_views: [wgpu::TextureView; CASCADE_COUNT],
    /// Staging buffer holding 3 cascade VP matrices (192 bytes, COPY_SRC).
    /// Updated each frame via `update_cascade_vps`, then copied into the uniform
    /// buffer's `sun_view_proj` slot before each cascade's shadow pass.
    pub vp_staging: wgpu::Buffer,
}

/// Create the cascaded shadow depth texture (3-layer 2D array) and its views.
pub fn create_shadow_texture(device: &wgpu::Device) -> ShadowCascades {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Shadow Cascades"),
        size: wgpu::Extent3d {
            width: SHADOW_MAP_SIZE,
            height: SHADOW_MAP_SIZE,
            depth_or_array_layers: CASCADE_COUNT as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("Shadow Array View"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });

    let cascade_views = std::array::from_fn(|i| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(&format!("Shadow Cascade {i}")),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: i as u32,
            array_layer_count: Some(1),
            ..Default::default()
        })
    });

    // Staging buffer for 3 cascade VP matrices (3 × 64 = 192 bytes)
    let vp_staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Cascade VP Staging"),
        size: (CASCADE_COUNT as u64) * MAT4_SIZE,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    ShadowCascades { _texture: texture, array_view, cascade_views, vp_staging }
}

/// Create the bind group layout for shadow sampling (cascade depth array + cloud shadow texture).
pub fn create_shadow_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Shadow BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Create the shadow bind group (cascade depth array + cloud shadow texture).
pub fn create_shadow_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    array_view: &wgpu::TextureView,
    cloud_shadow_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Shadow Sampler"),
        compare: Some(wgpu::CompareFunction::LessEqual),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let cloud_shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Cloud Shadow Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Shadow BG"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(array_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(cloud_shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&cloud_shadow_sampler),
            },
        ],
    })
}

/// Compute per-cascade orthographic view-projection matrices + split distances.
///
/// Returns `([vp0, vp1, vp2], [split0, split1, split2, 0.0])`.
pub fn compute_cascade_view_projs(
    sun_dir: glam::Vec3,
    camera_pos: glam::Vec3,
) -> ([glam::Mat4; CASCADE_COUNT], [f32; 4]) {
    let depth = 300.0;

    let up = if sun_dir.y.abs() > 0.99 {
        glam::Vec3::Z
    } else {
        glam::Vec3::Y
    };

    let sun_pos = camera_pos + sun_dir * (depth * 0.5);
    let sun_view = glam::Mat4::look_at_rh(sun_pos, camera_pos, up);

    let vps = std::array::from_fn(|i| {
        let extent = CASCADE_SPLITS[i];
        let proj = glam::Mat4::orthographic_rh(-extent, extent, -extent, extent, 0.1, depth);
        let shadow_vp = proj * sun_view;

        // Snap to texel grid to prevent shadow swimming when camera moves
        let texel_size = (extent * 2.0) / SHADOW_MAP_SIZE as f32;
        let origin_clip = shadow_vp * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
        // origin_clip.xy is in NDC [-1,1]; convert to texel coordinates
        let texel_x = origin_clip.x / texel_size;
        let texel_y = origin_clip.y / texel_size;
        let dx = (texel_x.round() - texel_x) * texel_size;
        let dy = (texel_y.round() - texel_y) * texel_size;

        let mut snapped = shadow_vp;
        snapped.w_axis.x += dx;
        snapped.w_axis.y += dy;
        snapped
    });

    let splits = [CASCADE_SPLITS[0], CASCADE_SPLITS[1], CASCADE_SPLITS[2], 0.0];
    (vps, splits)
}

/// Copy the appropriate cascade VP matrix from the staging buffer into `sun_view_proj`
/// within the uniform buffer.
///
/// Called before each cascade's shadow render pass so that `vs_shadow` reads the correct
/// view-projection matrix.
pub fn copy_cascade_vp(
    encoder: &mut wgpu::CommandEncoder,
    vp_staging: &wgpu::Buffer,
    uniform_buffer: &wgpu::Buffer,
    cascade_index: usize,
) {
    let src_offset = cascade_index as u64 * MAT4_SIZE;
    encoder.copy_buffer_to_buffer(vp_staging, src_offset, uniform_buffer, SUN_VP_OFFSET, MAT4_SIZE);
}

/// Write the 3 cascade VP matrices to the staging buffer.
/// Call once per frame before encoding.
pub fn update_cascade_vps(queue: &wgpu::Queue, staging: &wgpu::Buffer, vps: &[glam::Mat4; CASCADE_COUNT]) {
    let mut data = [0u8; CASCADE_COUNT * 64];
    for (i, vp) in vps.iter().enumerate() {
        let cols = vp.to_cols_array();
        let bytes = bytemuck::bytes_of(&cols);
        data[i * 64..(i + 1) * 64].copy_from_slice(bytes);
    }
    queue.write_buffer(staging, 0, &data);
}
