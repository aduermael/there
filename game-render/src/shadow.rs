use crate::DEPTH_FORMAT;

pub const SHADOW_MAP_SIZE: u32 = 512;

/// Create the shadow depth texture and its view.
pub fn create_shadow_texture(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Shadow Depth"),
        size: wgpu::Extent3d {
            width: SHADOW_MAP_SIZE,
            height: SHADOW_MAP_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Create the bind group layout for shadow map sampling (texture + comparison sampler).
pub fn create_shadow_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Shadow BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
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
        ],
    })
}

/// Create the shadow bind group (texture view + comparison sampler).
pub fn create_shadow_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    shadow_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Shadow Sampler"),
        compare: Some(wgpu::CompareFunction::LessEqual),
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
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

/// Compute sun orthographic view-projection matrix.
/// Covers a 200x200 unit area centered on the camera position.
pub fn compute_sun_view_proj(sun_dir: glam::Vec3, camera_pos: glam::Vec3) -> glam::Mat4 {
    let extent = 100.0;
    let depth = 200.0;

    // Sun "camera" positioned behind the scene along sun_dir
    let sun_pos = camera_pos + sun_dir * (depth * 0.5);

    // Stable up vector
    let up = if sun_dir.y.abs() > 0.99 {
        glam::Vec3::Z
    } else {
        glam::Vec3::Y
    };

    let sun_view = glam::Mat4::look_at_rh(sun_pos, camera_pos, up);
    let sun_proj = glam::Mat4::orthographic_rh(-extent, extent, -extent, extent, 0.1, depth);

    sun_proj * sun_view
}
