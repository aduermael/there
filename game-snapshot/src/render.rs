use game_render::{create_depth_texture, TerrainRenderer, Uniforms};
use wgpu::util::DeviceExt;

const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Compute sun direction from sun_angle (0.0=dawn, 0.25=noon, 0.5=dusk, 0.75=night, 1.0=dawn)
fn sun_direction_from_angle(sun_angle: f32) -> glam::Vec3 {
    let theta = sun_angle * std::f32::consts::TAU; // full orbit
    // Sun orbits east-west: x = cos(theta), y = sin(theta) (above horizon when y>0)
    let dir = glam::Vec3::new(theta.cos(), theta.sin(), 0.3).normalize();
    // At night (y < 0.05), clamp to just above horizon so lighting doesn't go fully black
    if dir.y < 0.05 {
        glam::Vec3::new(dir.x, 0.05, dir.z).normalize()
    } else {
        dir
    }
}

pub async fn render_frame(
    width: u32,
    height: u32,
    camera_pos: glam::Vec3,
    camera_target: glam::Vec3,
    sun_angle: f32,
) -> Vec<u8> {
    // --- Create headless wgpu device ---
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .expect("No suitable GPU adapter found");

    log::info!("GPU adapter: {:?}", adapter.get_info());

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Snapshot Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
        .await
        .expect("Failed to create device");

    // --- Offscreen render target ---
    let render_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Offscreen Target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = create_depth_texture(&device, width, height);

    // --- Heightmap ---
    let heightmap_data = game_core::terrain::generate_heightmap();
    let hm_res = game_core::HEIGHTMAP_RES;

    let heightmap_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Heightmap"),
        size: wgpu::Extent3d {
            width: hm_res,
            height: hm_res,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &heightmap_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&heightmap_data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(hm_res * 4),
            rows_per_image: Some(hm_res),
        },
        wgpu::Extent3d {
            width: hm_res,
            height: hm_res,
            depth_or_array_layers: 1,
        },
    );

    let heightmap_view = heightmap_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // --- Uniform buffer ---
    let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Uniform BGL"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let view = glam::Mat4::look_at_rh(camera_pos, camera_target, glam::Vec3::Y);
    let aspect = width as f32 / height as f32;
    let proj = glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 500.0);
    let view_proj = proj * view;
    let sun_dir = sun_direction_from_angle(sun_angle);

    let uniforms = Uniforms {
        view_proj: view_proj.to_cols_array(),
        camera_pos: camera_pos.to_array(),
        _pad0: 0.0,
        sun_dir: sun_dir.to_array(),
        _pad1: 0.0,
        fog_color: [0.53, 0.81, 0.92],
        fog_far: 300.0,
        world_size: game_core::WORLD_SIZE,
        hm_res: game_core::HEIGHTMAP_RES as f32,
        _pad2: [0.0; 2],
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Uniform BG"),
        layout: &uniform_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    // --- Terrain renderer ---
    let terrain = TerrainRenderer::new(
        &device,
        TEXTURE_FORMAT,
        &uniform_bgl,
        &heightmap_view,
        &heightmap_data,
    );

    // --- Render pass ---
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Snapshot Render"),
    });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Snapshot Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &render_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.53,
                        g: 0.81,
                        b: 0.92,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        terrain.draw(&mut pass, &uniform_bind_group, camera_pos, &view_proj);
    }

    // --- Pixel readback ---
    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = (unpadded_bytes_per_row + align - 1) / align * align;

    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Staging"),
        size: (padded_bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &render_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(std::iter::once(encoder.finish()));

    let buffer_slice = staging_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().expect("Buffer map failed");

    let data = buffer_slice.get_mapped_range();

    // Remove row padding
    let mut pixels = Vec::with_capacity((width * height * bytes_per_pixel) as usize);
    for row in 0..height {
        let start = (row * padded_bytes_per_row) as usize;
        let end = start + unpadded_bytes_per_row as usize;
        pixels.extend_from_slice(&data[start..end]);
    }

    pixels
}
