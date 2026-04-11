use game_render::{
    compute_atmosphere, compute_cascade_view_projs, create_depth_texture, create_shadow_bgl,
    create_shadow_bind_group, create_shadow_texture, encode_frame, update_cascade_vps,
    BloomRenderer, FxaaRenderer, GrassRenderer, PostProcessRenderer, RockRenderer,
    SceneRenderers, SkyRenderer, SsaoRenderer, TerrainRenderer, TextureAtlas, TreeRenderer,
    WaterRenderer, Uniforms, INTERMEDIATE_FORMAT,
};
// All instance renderers (grass, trees, rocks) use GPU compute; no CPU scatter needed.
use wgpu::util::DeviceExt;

const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

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
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
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
    let atmo = compute_atmosphere(sun_angle);

    let (cascade_vps, cascade_splits) = compute_cascade_view_projs(atmo.sun_dir, camera_pos);

    let uniforms = Uniforms {
        view_proj: view_proj.to_cols_array(),
        camera_pos: camera_pos.to_array(),
        _pad0: 0.0,
        sun_dir: atmo.sun_dir.to_array(),
        _pad1: 0.0,
        fog_color: atmo.fog_color,
        fog_density: atmo.fog_density,
        world_size: game_core::WORLD_SIZE,
        hm_res: game_core::HEIGHTMAP_RES as f32,
        fog_height_falloff: atmo.fog_height_falloff,
        time: 0.0,
        sun_color: atmo.sun_color,
        _pad3: 0.0,
        sky_zenith: atmo.sky_zenith,
        _pad4: 0.0,
        sky_horizon: atmo.sky_horizon,
        _pad5: 0.0,
        inv_view_proj: view_proj.inverse().to_cols_array(),
        sky_ambient: atmo.sky_ambient,
        _pad6: 0.0,
        ground_ambient: atmo.ground_ambient,
        _pad7: 0.0,
        sun_view_proj: cascade_vps[0].to_cols_array(),
        cascade_vp0: cascade_vps[0].to_cols_array(),
        cascade_vp1: cascade_vps[1].to_cols_array(),
        cascade_vp2: cascade_vps[2].to_cols_array(),
        cascade_splits,
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

    // --- Shadow resources ---
    let shadow_cascades = create_shadow_texture(&device);
    let shadow_bgl = create_shadow_bgl(&device);
    let shadow_bind_group = create_shadow_bind_group(&device, &shadow_bgl, &shadow_cascades.array_view);

    // --- Material texture atlas ---
    let atlas = TextureAtlas::new(&device, &queue);

    // --- Scene renderers (all target HDR intermediate) ---
    let terrain = TerrainRenderer::new(
        &device,
        INTERMEDIATE_FORMAT,
        &uniform_bgl,
        &shadow_bgl,
        &heightmap_view,
        &heightmap_data,
        &atlas.view,
        &atlas.sampler,
    );

    let sky = SkyRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl);
    let water = WaterRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &depth_view);

    let rock_renderer =
        RockRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &uniform_buffer, &heightmap_view, &atlas.bind_group_layout);
    let tree_renderer =
        TreeRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &uniform_buffer, &heightmap_view, &atlas.bind_group_layout);
    let grass_renderer =
        GrassRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &uniform_buffer, &heightmap_view);

    // --- SSAO renderer ---
    let ssao = SsaoRenderer::new(&device, &uniform_bgl, &depth_view, width, height);

    // --- Bloom renderer ---
    let mut bloom = BloomRenderer::new(&device, width, height);

    // --- Post-process renderer ---
    let postprocess = PostProcessRenderer::new(&device, TEXTURE_FORMAT, &uniform_bgl, ssao.ao_view(), &depth_view, bloom.result_view(), width, height);

    // Link bloom to HDR intermediate
    bloom.build_bind_groups(&device, postprocess.intermediate_view());

    // --- FXAA renderer ---
    let fxaa = FxaaRenderer::new(&device, TEXTURE_FORMAT, width, height);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Snapshot Render"),
    });

    let scene = SceneRenderers {
        terrain: &terrain,
        sky: &sky,
        water: &water,
        grass: &grass_renderer,
        rocks: &rock_renderer,
        trees: &tree_renderer,
        players: None,
        ssao: &ssao,
        bloom: &bloom,
        postprocess: &postprocess,
        fxaa: &fxaa,
    };

    // Write cascade VP matrices to staging buffer before encoding
    update_cascade_vps(&queue, &shadow_cascades.vp_staging, &cascade_vps);

    encode_frame(
        &mut encoder, &scene,
        &uniform_bind_group, &uniform_buffer,
        &shadow_bind_group, &shadow_cascades.cascade_views,
        &shadow_cascades.vp_staging,
        &depth_view, &render_view,
        camera_pos, &view_proj,
        &atlas.bind_group,
    );

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
