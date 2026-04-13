use game_render::{
    compute_atmosphere, compute_cascade_view_projs, create_depth_texture, create_shadow_bgl,
    create_shadow_bind_group, create_shadow_texture, encode_frame, update_cascade_vps,
    BlobShadowRenderer, BloomRenderer, ExposureRenderer, FxaaRenderer, GrassRenderer,
    PlayerRenderer, PlayerInstance, player_color, PostProcessRenderer, RockRenderer,
    SceneRenderers, ShadowCascades, SkyRenderer, SsaoRenderer, TerrainRenderer, TextureAtlas,
    TreeRenderer, WaterRenderer, Uniforms, INTERMEDIATE_FORMAT,
};
use wgpu::util::DeviceExt;

const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub struct PlayerOpts {
    pub pos: Option<glam::Vec3>,
    pub yaw: Option<f32>,
}

/// Holds all GPU resources needed for snapshot rendering.
/// Created once, supports multiple render_view() calls with different camera params.
struct SnapshotContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    width: u32,
    height: u32,
    render_texture: wgpu::Texture,
    render_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    #[allow(dead_code)]
    heightmap_data: Vec<f32>,
    #[allow(dead_code)]
    uniform_bgl: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    shadow_cascades: ShadowCascades,
    #[allow(dead_code)]
    shadow_bgl: wgpu::BindGroupLayout,
    shadow_bind_group: wgpu::BindGroup,
    atlas: TextureAtlas,
    terrain: TerrainRenderer,
    sky: SkyRenderer,
    water: WaterRenderer,
    rock_renderer: RockRenderer,
    tree_renderer: TreeRenderer,
    grass_renderer: GrassRenderer,
    ssao: SsaoRenderer,
    bloom: BloomRenderer,
    exposure: ExposureRenderer,
    postprocess: PostProcessRenderer,
    fxaa: FxaaRenderer,
    blob_shadow_renderer: Option<BlobShadowRenderer>,
    player_renderer: Option<PlayerRenderer>,
}

impl SnapshotContext {
    async fn new(
        width: u32,
        height: u32,
        sun_angle: f32,
        camera_pos: glam::Vec3,
        camera_target: glam::Vec3,
        player: Option<PlayerOpts>,
    ) -> Self {
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

        let render_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = create_depth_texture(&device, width, height);

        let heightmap_data = game_core::terrain::generate_heightmap();
        let hm_res = game_core::HEIGHTMAP_RES;

        let heightmap_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Heightmap"),
            size: wgpu::Extent3d { width: hm_res, height: hm_res, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &heightmap_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&heightmap_data),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(hm_res * 4), rows_per_image: Some(hm_res) },
            wgpu::Extent3d { width: hm_res, height: hm_res, depth_or_array_layers: 1 },
        );
        let heightmap_view = heightmap_texture.create_view(&wgpu::TextureViewDescriptor::default());

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

        // Create initial uniforms (will be overwritten per view)
        let atmo = compute_atmosphere(sun_angle);
        let view_mat = glam::Mat4::look_at_rh(camera_pos, camera_target, glam::Vec3::Y);
        let aspect = width as f32 / height as f32;
        let proj = glam::Mat4::perspective_rh(game_core::camera::FOV, aspect, game_core::camera::NEAR_PLANE, game_core::camera::FAR_PLANE);
        let view_proj = proj * view_mat;
        let (cascade_vps, cascade_splits) = compute_cascade_view_projs(atmo.sun_dir, camera_pos);

        let uniforms = Self::build_uniforms(camera_pos, &view_proj, &atmo, &cascade_vps, cascade_splits);

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform BG"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }],
        });

        let shadow_cascades = create_shadow_texture(&device);
        let shadow_bgl = create_shadow_bgl(&device);
        let shadow_bind_group = create_shadow_bind_group(&device, &shadow_bgl, &shadow_cascades.array_view);

        let atlas = TextureAtlas::new(&device, &queue);

        let terrain = TerrainRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &heightmap_view, &heightmap_data, &atlas.view, &atlas.sampler);
        let sky = SkyRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl);
        let water = WaterRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &depth_view);
        let rock_renderer = RockRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &uniform_buffer, &heightmap_view, &atlas.bind_group_layout);
        let tree_renderer = TreeRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &uniform_buffer, &heightmap_view, &atlas.bind_group_layout);
        let grass_renderer = GrassRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &uniform_buffer, &heightmap_view);

        let ssao = SsaoRenderer::new(&device, &uniform_bgl, &depth_view, width, height);
        let mut bloom = BloomRenderer::new(&device, width, height);
        let mut exposure = ExposureRenderer::new(&device, width, height);
        let postprocess = PostProcessRenderer::new(&device, TEXTURE_FORMAT, &uniform_bgl, ssao.ao_view(), &depth_view, bloom.result_view(), exposure.exposure_buffer(), width, height);
        bloom.build_bind_groups(&device, postprocess.intermediate_view());
        exposure.build_bind_groups(&device, postprocess.intermediate_view());
        let fxaa = FxaaRenderer::new(&device, TEXTURE_FORMAT, width, height);

        let blob_shadow_renderer = player.as_ref().map(|_| {
            BlobShadowRenderer::new(&device, TEXTURE_FORMAT, &uniform_bgl)
        });

        let player_renderer = player.map(|opts| {
            let pr = PlayerRenderer::new(&device, &queue, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl);

            let pos = match opts.pos {
                Some(p) => {
                    if p.y < 0.0 {
                        let y = game_core::terrain::sample_height(&heightmap_data, p.x, p.z);
                        glam::Vec3::new(p.x, y, p.z)
                    } else {
                        p
                    }
                }
                None => {
                    let y = game_core::terrain::sample_height(&heightmap_data, camera_target.x, camera_target.z);
                    glam::Vec3::new(camera_target.x, y, camera_target.z)
                }
            };

            let yaw = opts.yaw.unwrap_or_else(|| {
                let dx = camera_pos.x - pos.x;
                let dz = camera_pos.z - pos.z;
                (-dx).atan2(-dz)
            });

            let instance = PlayerInstance {
                pos_yaw: [pos.x, pos.y, pos.z, yaw],
                color: [player_color(0)[0], player_color(0)[1], player_color(0)[2], 0.0],
            };
            pr.update_instances(&queue, &[instance]);
            let bind_matrices = pr.skeleton().bind_pose_matrices();
            pr.upload_bones(&queue, 0, &bind_matrices);
            pr
        });

        Self {
            device, queue, width, height,
            render_texture, render_view, depth_view,
            heightmap_data, uniform_bgl, uniform_buffer, uniform_bind_group,
            shadow_cascades, shadow_bgl, shadow_bind_group,
            atlas, terrain, sky, water, rock_renderer, tree_renderer, grass_renderer,
            ssao, bloom, exposure, postprocess, fxaa,
            blob_shadow_renderer, player_renderer,
        }
    }

    fn build_uniforms(
        camera_pos: glam::Vec3,
        view_proj: &glam::Mat4,
        atmo: &game_render::AtmosphereParams,
        cascade_vps: &[glam::Mat4; 3],
        cascade_splits: [f32; 4],
    ) -> Uniforms {
        Uniforms {
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
        }
    }

    /// Render a single view with the given camera params, return pixels.
    fn render_view(&self, camera_pos: glam::Vec3, camera_target: glam::Vec3, sun_angle: f32) -> Vec<u8> {
        let atmo = compute_atmosphere(sun_angle);
        let view_mat = glam::Mat4::look_at_rh(camera_pos, camera_target, glam::Vec3::Y);
        let aspect = self.width as f32 / self.height as f32;
        let proj = glam::Mat4::perspective_rh(game_core::camera::FOV, aspect, game_core::camera::NEAR_PLANE, game_core::camera::FAR_PLANE);
        let view_proj = proj * view_mat;
        let (cascade_vps, cascade_splits) = compute_cascade_view_projs(atmo.sun_dir, camera_pos);

        let uniforms = Self::build_uniforms(camera_pos, &view_proj, &atmo, &cascade_vps, cascade_splits);
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Snapshot Render"),
        });

        let scene = SceneRenderers {
            terrain: &self.terrain,
            sky: &self.sky,
            water: &self.water,
            grass: &self.grass_renderer,
            rocks: &self.rock_renderer,
            trees: &self.tree_renderer,
            blob_shadow: self.blob_shadow_renderer.as_ref(),
            players: self.player_renderer.as_ref(),
            ssao: &self.ssao,
            bloom: &self.bloom,
            exposure: &self.exposure,
            postprocess: &self.postprocess,
            fxaa: &self.fxaa,
        };

        update_cascade_vps(&self.queue, &self.shadow_cascades.vp_staging, &cascade_vps);

        encode_frame(
            &mut encoder, &scene,
            &self.uniform_bind_group, &self.uniform_buffer,
            &self.shadow_bind_group, &self.shadow_cascades.cascade_views,
            &self.shadow_cascades.vp_staging,
            &self.depth_view, &self.render_view,
            camera_pos, &view_proj,
            &self.atlas.bind_group,
        );

        // Pixel readback
        let bytes_per_pixel = 4u32;
        let unpadded_bytes_per_row = self.width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = (unpadded_bytes_per_row + align - 1) / align * align;

        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging"),
            size: (padded_bytes_per_row * self.height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.render_texture, mip_level: 0,
                origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = staging_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().expect("Buffer map failed");

        let data = buffer_slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((self.width * self.height * bytes_per_pixel) as usize);
        for row in 0..self.height {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + unpadded_bytes_per_row as usize;
            pixels.extend_from_slice(&data[start..end]);
        }
        pixels
    }
}

/// Simulation renderer — wraps SnapshotContext for step-based simulation.
/// Supports updating the player position/yaw and rendering snapshots at each step.
pub struct SimRenderer {
    ctx: SnapshotContext,
    orbit_pitch: f32,
    orbit_distance: f32,
    sun_angle: f32,
}

impl SimRenderer {
    pub async fn new(
        width: u32,
        height: u32,
        sun_angle: f32,
        player_pos: glam::Vec3,
        orbit_yaw: f32,
        orbit_pitch: f32,
        orbit_distance: f32,
    ) -> Self {
        let (eye, target) = game_core::camera::orbit_eye(player_pos, orbit_yaw, orbit_pitch, orbit_distance);
        let player_opts = Some(PlayerOpts {
            pos: Some(player_pos),
            yaw: Some(0.0),
        });
        let ctx = SnapshotContext::new(width, height, sun_angle, eye, target, player_opts).await;
        Self { ctx, orbit_pitch, orbit_distance, sun_angle }
    }

    pub fn heightmap(&self) -> &[f32] {
        &self.ctx.heightmap_data
    }

    pub fn update_player(&self, pos: glam::Vec3, yaw: f32) {
        if let Some(pr) = &self.ctx.player_renderer {
            let instance = PlayerInstance {
                pos_yaw: [pos.x, pos.y, pos.z, yaw],
                color: [player_color(0)[0], player_color(0)[1], player_color(0)[2], 0.0],
            };
            pr.update_instances(&self.ctx.queue, &[instance]);
        }
    }

    pub fn snapshot(&self, player_pos: glam::Vec3, orbit_yaw: f32, output: &str) {
        let (eye, target) = game_core::camera::orbit_eye(
            player_pos, orbit_yaw, self.orbit_pitch, self.orbit_distance,
        );
        let pixels = self.ctx.render_view(eye, target, self.sun_angle);
        image::save_buffer(output, &pixels, self.ctx.width, self.ctx.height, image::ColorType::Rgba8)
            .expect("Failed to save snapshot PNG");
        log::info!("Saved snapshot: {}", output);
    }
}

/// Render a single frame (backward-compatible entry point).
pub async fn render_frame(
    width: u32,
    height: u32,
    camera_pos: glam::Vec3,
    camera_target: glam::Vec3,
    sun_angle: f32,
    player: Option<PlayerOpts>,
) -> Vec<u8> {
    let ctx = SnapshotContext::new(width, height, sun_angle, camera_pos, camera_target, player).await;
    ctx.render_view(camera_pos, camera_target, sun_angle)
}

/// Render a turntable grid: 8 orbit views composited into one image.
pub async fn render_turntable(
    width: u32,
    height: u32,
    player_pos: glam::Vec3,
    orbit_pitch: f32,
    orbit_distance: f32,
    sun_angle: f32,
    cols: u32,
) -> Vec<u8> {
    let frames = 8u32;
    let rows = (frames + cols - 1) / cols;
    let sub_w = width / cols;
    let sub_h = height / rows;

    // Use first yaw angle for initial context setup
    let (init_eye, init_target) = game_core::camera::orbit_eye(player_pos, 0.0, orbit_pitch, orbit_distance);

    let player_opts = Some(PlayerOpts {
        pos: Some(player_pos),
        yaw: None, // face toward camera (default)
    });

    let ctx = SnapshotContext::new(sub_w, sub_h, sun_angle, init_eye, init_target, player_opts).await;

    // Render 8 views at evenly spaced yaw angles
    let mut sub_frames: Vec<Vec<u8>> = Vec::with_capacity(frames as usize);
    for i in 0..frames {
        let yaw = i as f32 * std::f32::consts::TAU / frames as f32;
        let (eye, target) = game_core::camera::orbit_eye(player_pos, yaw, orbit_pitch, orbit_distance);
        sub_frames.push(ctx.render_view(eye, target, sun_angle));
    }

    // Composite into grid
    let bpp = 4u32;
    let mut output = vec![0u8; (width * height * bpp) as usize];

    for (i, sub_pixels) in sub_frames.iter().enumerate() {
        let grid_col = i as u32 % cols;
        let grid_row = i as u32 / cols;
        let x_off = grid_col * sub_w;
        let y_off = grid_row * sub_h;

        for sy in 0..sub_h {
            let src_start = (sy * sub_w * bpp) as usize;
            let src_end = src_start + (sub_w * bpp) as usize;
            let dst_start = ((y_off + sy) * width * bpp + x_off * bpp) as usize;
            let dst_end = dst_start + (sub_w * bpp) as usize;
            output[dst_start..dst_end].copy_from_slice(&sub_pixels[src_start..src_end]);
        }
    }

    output
}
