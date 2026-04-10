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
