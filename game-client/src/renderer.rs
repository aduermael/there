use wgpu::util::DeviceExt;
use web_sys::HtmlCanvasElement;

use game_render::{
    PlayerInstance, PlayerRenderer, RockRenderer, SkyRenderer, TerrainRenderer, TreeRenderer,
    Uniforms, create_depth_texture, scatter_objects,
};

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_view: wgpu::TextureView,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    sky: SkyRenderer,
    terrain: TerrainRenderer,
    players: PlayerRenderer,
    rocks: RockRenderer,
    trees: TreeRenderer,
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

        // Uniform buffer + bind group (shared across pipelines)
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

        // Sky renderer
        let sky = SkyRenderer::new(&device, format, &uniform_bgl);

        // Terrain renderer
        let terrain =
            TerrainRenderer::new(&device, format, &uniform_bgl, &heightmap_view, heightmap_data);

        // Player renderer
        let players = PlayerRenderer::new(&device, format, &uniform_bgl);

        // Scene objects (rocks + trees)
        let (rock_instances, tree_instances) = scatter_objects(heightmap_data);
        let rocks = RockRenderer::new(&device, &queue, format, &uniform_bgl, &rock_instances);
        let trees = TreeRenderer::new(&device, &queue, format, &uniform_bgl, &tree_instances);

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
            uniform_buffer,
            uniform_bind_group,
            sky,
            terrain,
            players,
            rocks,
            trees,
        }
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn update_uniforms(&self, uniforms: &Uniforms) {
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.depth_view = create_depth_texture(&self.device, width, height);
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

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            self.sky.draw(&mut pass, &self.uniform_bind_group);
            self.terrain
                .draw(&mut pass, &self.uniform_bind_group, camera_pos, view_proj);
            self.rocks.draw(&mut pass, &self.uniform_bind_group);
            self.trees.draw(&mut pass, &self.uniform_bind_group);
            self.players.draw(
                &mut pass,
                &self.uniform_bind_group,
                player_instances.len() as u32,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
