use wgpu::util::DeviceExt;
use web_sys::HtmlCanvasElement;

use game_render::{
    BloomRenderer, FxaaRenderer, GrassRenderer, PlayerInstance, PlayerRenderer,
    PostProcessRenderer, RockRenderer, SceneRenderers, ShadowCascades, SkyRenderer, SsaoRenderer,
    TerrainRenderer, TextureAtlas, TreeRenderer, Uniforms, create_depth_texture, create_shadow_bgl,
    create_shadow_bind_group, create_shadow_texture, encode_frame,
    INTERMEDIATE_FORMAT,
};

// All instance renderers (grass, trees, rocks) use GPU compute; no CPU scatter needed.

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_view: wgpu::TextureView,
    shadow_cascades: ShadowCascades,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    shadow_bind_group: wgpu::BindGroup,
    atlas: TextureAtlas,
    sky: SkyRenderer,
    terrain: TerrainRenderer,
    players: PlayerRenderer,
    rocks: RockRenderer,
    trees: TreeRenderer,
    grass: GrassRenderer,
    ssao: SsaoRenderer,
    bloom: BloomRenderer,
    postprocess: PostProcessRenderer,
    fxaa: FxaaRenderer,
}

impl Renderer {
    pub async fn new(canvas: HtmlCanvasElement, heightmap_data: &[f32]) -> Self {
        let window = web_sys::window().unwrap();
        let dpr = window.device_pixel_ratio();
        let width = (canvas.client_width() as f64 * dpr) as u32;
        let height = (canvas.client_height() as f64 * dpr) as u32;
        canvas.set_width(width);
        canvas.set_height(height);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .expect("Failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("No suitable GPU adapter found");

        log::info!("GPU adapter: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Game Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Heightmap texture (R32Float)
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
            bytemuck::cast_slice(heightmap_data),
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

        let heightmap_view =
            heightmap_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Depth texture
        let depth_view = create_depth_texture(&device, width, height);

        // Shadow cascade texture + bind group
        let shadow_cascades = create_shadow_texture(&device);
        let shadow_bgl = create_shadow_bgl(&device);
        let shadow_bind_group = create_shadow_bind_group(&device, &shadow_bgl, &shadow_cascades.array_view);

        // Uniform buffer + bind group (shared across pipelines)
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

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniforms"),
            contents: &[0u8; std::mem::size_of::<Uniforms>()],
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

        // Material texture atlas
        let atlas = TextureAtlas::new(&device, &queue);

        // Scene renderers (all target HDR intermediate)
        let sky = SkyRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl);
        let terrain =
            TerrainRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &heightmap_view, heightmap_data, &atlas.view, &atlas.sampler);
        let players = PlayerRenderer::new(&device, &queue, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl);

        let rocks = RockRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &uniform_buffer, &heightmap_view);
        let trees = TreeRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &uniform_buffer, &heightmap_view);
        let grass = GrassRenderer::new(&device, INTERMEDIATE_FORMAT, &uniform_bgl, &shadow_bgl, &uniform_buffer, &heightmap_view);

        // SSAO renderer
        let ssao = SsaoRenderer::new(&device, &uniform_bgl, &depth_view, width, height);

        // Bloom renderer (compute, needs HDR view from postprocess)
        let mut bloom = BloomRenderer::new(&device, width, height);

        // Post-process renderer (HDR intermediate → surface)
        let postprocess = PostProcessRenderer::new(&device, format, &uniform_bgl, ssao.ao_view(), &depth_view, bloom.result_view(), width, height);

        // Link bloom to HDR intermediate (created by postprocess)
        bloom.build_bind_groups(&device, postprocess.intermediate_view());

        // FXAA renderer (postprocess → LDR intermediate → FXAA → surface)
        let fxaa = FxaaRenderer::new(&device, format, width, height);

        log::info!(
            "Renderer initialized: {}x{}, format={:?}",
            width,
            height,
            format
        );

        Self {
            surface,
            device,
            queue,
            config,
            depth_view,
            shadow_cascades,
            uniform_buffer,
            uniform_bind_group,
            shadow_bind_group,
            atlas,
            sky,
            terrain,
            players,
            rocks,
            trees,
            grass,
            ssao,
            bloom,
            postprocess,
            fxaa,
        }
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn update_uniforms(&self, uniforms: &Uniforms) {
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));
    }

    pub fn update_cascade_vps(&self, vps: &[glam::Mat4; 3]) {
        game_render::update_cascade_vps(&self.queue, &self.shadow_cascades.vp_staging, vps);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.depth_view = create_depth_texture(&self.device, width, height);
            self.ssao.resize(&self.device, &self.depth_view, width, height);
            self.bloom.resize(&self.device, width, height);
            self.postprocess.resize(&self.device, self.ssao.ao_view(), &self.depth_view, self.bloom.result_view(), width, height);
            self.bloom.build_bind_groups(&self.device, self.postprocess.intermediate_view());
            self.fxaa.resize(&self.device, width, height);
        }
    }

    pub fn render(
        &self,
        camera_pos: glam::Vec3,
        view_proj: &glam::Mat4,
        player_instances: &[PlayerInstance],
    ) {
        self.players
            .update_instances(&self.queue, player_instances);
        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {:?}", e);
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render"),
            });

        let scene = SceneRenderers {
            terrain: &self.terrain,
            sky: &self.sky,
            grass: &self.grass,
            rocks: &self.rocks,
            trees: &self.trees,
            players: Some(&self.players),
            ssao: &self.ssao,
            bloom: &self.bloom,
            postprocess: &self.postprocess,
            fxaa: &self.fxaa,
        };

        encode_frame(
            &mut encoder, &scene,
            &self.uniform_bind_group, &self.uniform_buffer,
            &self.shadow_bind_group, &self.shadow_cascades.cascade_views,
            &self.shadow_cascades.vp_staging,
            &self.depth_view, &view,
            camera_pos, view_proj,
        );

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
