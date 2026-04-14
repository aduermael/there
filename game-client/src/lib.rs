use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

mod camera;
mod input;
mod net;
mod renderer;

use camera::OrbitCamera;
use game_core::protocol::{PlayerId, ServerMsg};
use game_core::{AnimState, WALK_ENTER_SPEED, WALK_EXIT_SPEED};
use game_render::animation::AnimationPlayer;
use game_render::{compute_atmosphere, compute_cascade_view_projs, player_color, PlayerInstance, Uniforms};
use input::InputState;
use net::Connection;
use renderer::Renderer;

thread_local! {
    static GLOBAL_CONN: RefCell<Option<Connection>> = RefCell::new(None);
    static PENDING_LOCAL_BUBBLES: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

#[wasm_bindgen]
pub fn send_chat(text: &str) {
    GLOBAL_CONN.with(|c| {
        if let Some(conn) = c.borrow().as_ref() {
            conn.send_chat(text);
        }
    });
}

#[wasm_bindgen]
pub fn add_local_chat_bubble(text: &str) {
    PENDING_LOCAL_BUBBLES.with(|b| {
        b.borrow_mut().push(text.to_string());
    });
}

#[wasm_bindgen]
pub fn send_player_name(name: &str) {
    GLOBAL_CONN.with(|c| {
        if let Some(conn) = c.borrow().as_ref() {
            conn.send_name(name);
        }
    });
}

#[wasm_bindgen(inline_js = "
export function hud_set_room(code) {
    const el = document.querySelector('game-hud');
    if (el) el.roomCode = code;
}
export function hud_set_players(n) {
    const el = document.querySelector('game-hud');
    if (el) el.playerCount = n;
}
export function hud_set_fps(fps) {
    const el = document.querySelector('game-hud');
    if (el) el.fps = fps;
}
export function js_is_daylight_cycle() {
    return !!window.__daylightCycle;
}
export function js_get_sun_angle() {
    return window.__sunAngle ?? 0.0;
}
export function js_set_sun_angle(a) {
    window.__sunAngle = a;
}
export function js_is_menu_open() {
    return !!window.__menuOpen;
}
export function js_set_room_code(code) {
    window.__roomCode = code;
}
export function js_chat_received(id, text) {
    window.dispatchEvent(new CustomEvent('chat-received', { detail: { id, text } }));
}
export function js_update_chat_bubbles(json) {
    window.dispatchEvent(new CustomEvent('chat-bubbles-update', { detail: json }));
}
export function js_names_updated(json) {
    window.dispatchEvent(new CustomEvent('player-names-updated', { detail: json }));
}
export function js_viewport_width() { return window.innerWidth; }
export function js_viewport_height() { return window.innerHeight; }
")]
extern "C" {
    fn hud_set_room(code: &str);
    fn hud_set_players(n: u32);
    fn hud_set_fps(fps: u32);
    fn js_is_daylight_cycle() -> bool;
    fn js_get_sun_angle() -> f32;
    fn js_set_sun_angle(a: f32);
    fn js_is_menu_open() -> bool;
    fn js_set_room_code(code: &str);
    fn js_chat_received(id: u16, text: &str);
    fn js_update_chat_bubbles(json: &str);
    fn js_names_updated(json: &str);
    fn js_viewport_width() -> f32;
    fn js_viewport_height() -> f32;
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            _ => out.push(c),
        }
    }
    out
}

struct ChatBubble {
    player_id: PlayerId,
    text: String,
    timestamp: f64,
}

struct RemotePlayer {
    prev: [f32; 4],   // [x, y, z, yaw] from previous snapshot
    target: [f32; 4], // [x, y, z, yaw] from latest snapshot
    recv_time: f64,    // ms timestamp when target was received
    server_anim_state: u8, // animation state from server
    anim: AnimationPlayer,
}

struct GameState {
    renderer: Renderer,
    camera: OrbitCamera,
    heightmap_data: Vec<f32>,
    players: Vec<PlayerInstance>,
    remotes: HashMap<PlayerId, RemotePlayer>,
    input: InputState,
    local_pos: glam::Vec3,
    local_player_id: Option<PlayerId>,
    last_send_time: f64,
    last_frame_time: f64,
    time: f32,
    frame_count: u32,
    fps_accum: f32,
    sun_angle: f32,
    cycle_active: bool,
    vertical_velocity: f32,
    jump_sent: bool,
    local_anim: AnimationPlayer,
    prev_local_pos: glam::Vec3,
    local_move_yaw: f32,
    local_visual_yaw: f32,
    active_bubbles: Vec<ChatBubble>,
    had_active_bubbles: bool,
    player_names: HashMap<PlayerId, String>,
}

impl GameState {
    fn process_server_messages(&mut self, messages: Vec<ServerMsg>, now: f64) {
        for msg in messages {
            match msg {
                ServerMsg::Welcome { your_id } => {
                    log::info!("Assigned player ID: {your_id}");
                    self.local_player_id = Some(your_id);
                    self.remotes.clear();
                    let c = player_color(your_id);
                    if let Some(p) = self.players.first_mut() {
                        p.color = [c[0], c[1], c[2], 0.0];
                    }
                }
                ServerMsg::Snapshot { players } => {
                    hud_set_players(players.len() as u32);
                    let local_id = self.local_player_id;

                    let mut seen = std::collections::HashSet::new();

                    for ps in &players {
                        if Some(ps.id) == local_id {
                            let server_pos = glam::Vec3::new(ps.x, ps.y, ps.z);
                            let delta = server_pos - self.local_pos;
                            let dist = delta.length();
                            if dist > 5.0 {
                                self.local_pos = server_pos;
                            } else if dist > 0.1 {
                                self.local_pos += delta * 0.3;
                            }
                            continue;
                        }
                        seen.insert(ps.id);
                        let new_pos = [ps.x, ps.y, ps.z, ps.yaw];
                        if let Some(rp) = self.remotes.get_mut(&ps.id) {
                            rp.prev = rp.target;
                            rp.target = new_pos;
                            rp.recv_time = now;
                            rp.server_anim_state = ps.anim_state;
                        } else {
                            self.remotes.insert(ps.id, RemotePlayer {
                                prev: new_pos,
                                target: new_pos,
                                recv_time: now,
                                server_anim_state: ps.anim_state,
                                anim: AnimationPlayer::new(),
                            });
                        }
                    }

                    self.remotes.retain(|id, _| seen.contains(id));
                }
                ServerMsg::PlayerLeft { id } => {
                    log::info!("Player {id} left");
                    self.remotes.remove(&id);
                }
                ServerMsg::Chat { from, text } => {
                    js_chat_received(from, &text);
                    self.active_bubbles.retain(|b| b.player_id != from);
                    self.active_bubbles.push(ChatBubble {
                        player_id: from,
                        text,
                        timestamp: now,
                    });
                }
                ServerMsg::NameUpdate { names } => {
                    self.player_names.clear();
                    let mut json_parts: Vec<String> = Vec::new();
                    for (id, name) in &names {
                        self.player_names.insert(*id, name.clone());
                        json_parts.push(format!(
                            r#"[{},"{}"]"#,
                            id,
                            json_escape(name)
                        ));
                    }
                    let json = format!("[{}]", json_parts.join(","));
                    js_names_updated(&json);
                }
            }
        }
    }

    fn update_movement(&mut self, dt: f32) {
        let menu_open = js_is_menu_open();

        let forward = if menu_open { 0.0 } else { self.input.forward() };
        let strafe = if menu_open { 0.0 } else { self.input.strafe() };
        let jump_pressed = if menu_open { false } else { self.input.jump_pressed() };
        let yaw = self.camera.yaw;

        let airborne = self.vertical_velocity != 0.0;

        // XZ movement
        if forward != 0.0 || strafe != 0.0 {
            let saved_y = self.local_pos.y;
            self.local_pos = game_core::movement::apply_movement(
                self.local_pos,
                forward,
                strafe,
                yaw,
                dt,
                &self.heightmap_data,
            );
            if airborne {
                // Preserve Y when airborne — vertical physics handles it
                self.local_pos.y = saved_y;
            }
            // else: Y = terrain height from apply_movement (normal ground tracking)
        }

        // Vertical physics — only when jumping or airborne
        let terrain_y = game_core::terrain::sample_height(
            &self.heightmap_data,
            self.local_pos.x,
            self.local_pos.z,
        );
        if jump_pressed || airborne {
            let (new_y, new_vel) = game_core::movement::apply_vertical(
                self.local_pos.y,
                self.vertical_velocity,
                terrain_y,
                jump_pressed,
                dt,
            );
            self.local_pos.y = new_y;
            self.vertical_velocity = new_vel;
        } else {
            // Grounded: snap to terrain
            self.local_pos.y = terrain_y;
        }

        // Latch jump for network send
        if jump_pressed {
            self.jump_sent = true;
        }

        self.camera.target = self.local_pos;
    }

    fn update_camera(&mut self, dt: f32) {
        self.camera.update(dt, &self.heightmap_data);
    }

    fn build_player_instances(&mut self, now: f64, dt: f32) {
        self.players.clear();

        // Smoothly rotate local visual yaw toward move_yaw (shortest arc)
        let turn_speed = game_core::PLAYER_TURN_SPEED;
        let max_step = turn_speed * dt.max(1.0 / 60.0);
        self.local_visual_yaw = game_core::movement::lerp_angle(self.local_visual_yaw, self.local_move_yaw, max_step);

        let yaw = self.local_visual_yaw;
        let local_color = self
            .local_player_id
            .map(|id| player_color(id))
            .unwrap_or([0.90, 0.30, 0.25]);
        let pos = self.local_pos;
        self.players.push(PlayerInstance {
            pos_yaw: [pos.x, pos.y, pos.z, yaw],
            color: [local_color[0], local_color[1], local_color[2], 0.0],
        });

        // Local player animation (with hysteresis to prevent Idle↔Walk flicker)
        let safe_dt = if dt > 0.0 { dt } else { 1.0 / 60.0 };
        let local_vel = (self.local_pos - self.prev_local_pos) / safe_dt;
        let horiz_speed = glam::Vec2::new(local_vel.x, local_vel.z).length();
        let mut anim_state = AnimState::from_movement(horiz_speed, self.vertical_velocity);
        // Hysteresis: require speed > WALK_ENTER_SPEED to enter Walk, < WALK_EXIT_SPEED to exit
        let cur = self.local_anim.current_state();
        if cur == AnimState::Idle && anim_state == AnimState::Walk && horiz_speed < WALK_ENTER_SPEED {
            anim_state = AnimState::Idle;
        } else if cur == AnimState::Walk && anim_state == AnimState::Idle && horiz_speed > WALK_EXIT_SPEED {
            anim_state = AnimState::Walk;
        }
        self.local_anim.set_state(anim_state);
        let local_pose = self.local_anim.update(dt);
        let skeleton = self.renderer.player_skeleton();
        let local_matrices = skeleton.compute_skin_matrices(&local_pose);
        self.renderer.upload_player_bones(0, &local_matrices);
        self.prev_local_pos = self.local_pos;

        let tick_ms = (game_core::TICK_INTERVAL_SECS * 1000.0) as f64;
        let mut instance_idx = 1usize;
        for (&id, rp) in self.remotes.iter_mut() {
            let t = ((now - rp.recv_time) / tick_ms).clamp(0.0, 1.0) as f32;
            let x = rp.prev[0] + (rp.target[0] - rp.prev[0]) * t;
            let y = rp.prev[1] + (rp.target[1] - rp.prev[1]) * t;
            let z = rp.prev[2] + (rp.target[2] - rp.prev[2]) * t;
            let mut dyaw = rp.target[3] - rp.prev[3];
            if dyaw > std::f32::consts::PI {
                dyaw -= std::f32::consts::TAU;
            } else if dyaw < -std::f32::consts::PI {
                dyaw += std::f32::consts::TAU;
            }
            let interp_yaw = rp.prev[3] + dyaw * t;

            // Use server-authoritative animation state for remote players
            let remote_anim = AnimState::from_u8(rp.server_anim_state);
            rp.anim.set_state(remote_anim);
            let pose = rp.anim.update(dt);
            let matrices = skeleton.compute_skin_matrices(&pose);
            self.renderer.upload_player_bones(instance_idx, &matrices);

            let c = player_color(id);
            self.players.push(PlayerInstance {
                pos_yaw: [x, y, z, interp_yaw],
                color: [c[0], c[1], c[2], 0.0],
            });
            instance_idx += 1;
        }
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    console_log::init_with_level(log::Level::Info).ok();
    log::info!("game-client WASM loaded");
    wasm_bindgen_futures::spawn_local(run());
}

async fn run() {
    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let canvas = document
        .get_element_by_id("game-canvas")
        .expect("no canvas element")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("not a canvas");

    let heightmap_data = game_core::terrain::generate_heightmap();
    log::info!("Heightmap generated: {} values", heightmap_data.len());

    let renderer = Renderer::new(canvas.clone(), &heightmap_data).await;

    let (spawn_x, spawn_z) = game_core::terrain::find_clear_spawn(&heightmap_data);
    let spawn_y = game_core::terrain::sample_height(&heightmap_data, spawn_x, spawn_z);
    let local_pos = glam::Vec3::new(spawn_x, spawn_y, spawn_z);

    let camera = OrbitCamera::new(
        local_pos,
        0.5, // yaw
        game_core::camera::DEFAULT_PITCH,
        game_core::camera::DEFAULT_DISTANCE,
    );

    let local_player = PlayerInstance {
        pos_yaw: [local_pos.x, local_pos.y, local_pos.z, 0.0],
        color: [0.90, 0.30, 0.25, 0.0],
    };

    // WebSocket connection — extract room code from URL path
    let incoming: Rc<RefCell<Vec<ServerMsg>>> = Rc::new(RefCell::new(Vec::new()));
    let pathname = window.location().pathname().unwrap_or_default();
    let room_code = pathname.trim_start_matches('/');
    let connection = if !room_code.is_empty() {
        log::info!("Room code: {room_code}");
        js_set_room_code(room_code);
        hud_set_room(room_code);
        let conn = Connection::new(room_code, incoming.clone());
        GLOBAL_CONN.with(|c| *c.borrow_mut() = Some(conn.clone()));
        Some(conn)
    } else {
        log::info!("No room code — solo mode");
        js_set_room_code("");
        None
    };

    let state = Rc::new(RefCell::new(GameState {
        renderer,
        camera,
        heightmap_data,
        players: vec![local_player],
        remotes: HashMap::new(),
        input: InputState::new(),
        local_pos,
        local_player_id: None,
        last_send_time: 0.0,
        last_frame_time: js_sys::Date::now(),
        time: 0.0,
        frame_count: 0,
        fps_accum: 0.0,
        sun_angle: 0.0,
        cycle_active: true,
        vertical_velocity: 0.0,
        jump_sent: false,
        local_anim: AnimationPlayer::new(),
        prev_local_pos: local_pos,
        local_move_yaw: 0.0,
        local_visual_yaw: 0.0,
        active_bubbles: Vec::new(),
        had_active_bubbles: false,
        player_names: HashMap::new(),
    }));

    // Solo mode: set local player ID to 0 so chat bubbles work
    if connection.is_none() {
        state.borrow_mut().local_player_id = Some(0);
    }

    // Initialize daylight globals for JS menu access
    js_set_sun_angle(0.0);

    setup_input(&canvas, state.clone());
    start_render_loop(state, connection, incoming);
}

fn setup_input(canvas: &web_sys::HtmlCanvasElement, state: Rc<RefCell<GameState>>) {
    let window = web_sys::window().unwrap();

    // Keyboard down
    let s = state.clone();
    let on_keydown = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
        if e.repeat() {
            return;
        }
        if s.borrow_mut().input.on_key_down(&e.code()) {
            e.prevent_default();
        }
    }) as Box<dyn FnMut(_)>);
    window
        .add_event_listener_with_callback("keydown", on_keydown.as_ref().unchecked_ref())
        .unwrap();
    on_keydown.forget();

    // Keyboard up
    let s = state.clone();
    let on_keyup = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
        s.borrow_mut().input.on_key_up(&e.code());
    }) as Box<dyn FnMut(_)>);
    window
        .add_event_listener_with_callback("keyup", on_keyup.as_ref().unchecked_ref())
        .unwrap();
    on_keyup.forget();

    // Pointer down (mouse only — touch camera handled by camera-control component)
    let s = state.clone();
    let on_down = Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
        if e.pointer_type() == "touch" {
            return;
        }
        s.borrow_mut()
            .camera
            .on_pointer_down(e.client_x() as f32, e.client_y() as f32);
    }) as Box<dyn FnMut(_)>);
    canvas
        .add_event_listener_with_callback("pointerdown", on_down.as_ref().unchecked_ref())
        .unwrap();
    on_down.forget();

    // Pointer move
    let s = state.clone();
    let on_move = Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
        s.borrow_mut()
            .camera
            .on_pointer_move(e.client_x() as f32, e.client_y() as f32);
    }) as Box<dyn FnMut(_)>);
    canvas
        .add_event_listener_with_callback("pointermove", on_move.as_ref().unchecked_ref())
        .unwrap();
    on_move.forget();

    // Pointer up (on window to catch releases outside canvas)
    let s = state.clone();
    let on_up = Closure::wrap(Box::new(move |_e: web_sys::PointerEvent| {
        s.borrow_mut().camera.on_pointer_up();
    }) as Box<dyn FnMut(_)>);
    window
        .add_event_listener_with_callback("pointerup", on_up.as_ref().unchecked_ref())
        .unwrap();
    on_up.forget();

    // Wheel (zoom)
    let s = state.clone();
    let on_wheel = Closure::wrap(Box::new(move |e: web_sys::WheelEvent| {
        s.borrow_mut().camera.on_wheel(e.delta_y() as f32);
    }) as Box<dyn FnMut(_)>);
    canvas
        .add_event_listener_with_callback("wheel", on_wheel.as_ref().unchecked_ref())
        .unwrap();
    on_wheel.forget();
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("requestAnimationFrame failed");
}

fn start_render_loop(
    state: Rc<RefCell<GameState>>,
    connection: Option<Connection>,
    incoming: Rc<RefCell<Vec<ServerMsg>>>,
) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::new(move || {
        {
            let mut state = state.borrow_mut();

            // Frame delta time
            let now = js_sys::Date::now();
            let dt = ((now - state.last_frame_time) / 1000.0) as f32;
            let dt = dt.clamp(0.0, 0.1);
            state.last_frame_time = now;
            state.time += dt;

            // FPS calculation (0.5s rolling average)
            state.frame_count += 1;
            state.fps_accum += dt;
            if state.fps_accum >= 0.5 {
                let fps = (state.frame_count as f32 / state.fps_accum).round() as u32;
                hud_set_fps(fps);
                state.frame_count = 0;
                state.fps_accum = 0.0;
            }

            // Daylight cycle
            state.cycle_active = js_is_daylight_cycle();
            if state.cycle_active {
                state.sun_angle = (state.sun_angle + dt / game_core::DAYLIGHT_CYCLE_SECS) % 1.0;
                js_set_sun_angle(state.sun_angle);
            } else {
                state.sun_angle = js_get_sun_angle();
            }

            // Process messages → movement → build instances
            let messages: Vec<ServerMsg> = incoming.borrow_mut().drain(..).collect();
            state.process_server_messages(messages, now);

            // Drain pending local chat bubbles (solo mode)
            PENDING_LOCAL_BUBBLES.with(|b| {
                let mut pending = b.borrow_mut();
                for text in pending.drain(..) {
                    let player_id = state.local_player_id.unwrap_or(0);
                    state.active_bubbles.retain(|b| b.player_id != player_id);
                    state.active_bubbles.push(ChatBubble {
                        player_id,
                        text,
                        timestamp: now,
                    });
                }
            });

            // Compute input
            let menu_open = js_is_menu_open();
            let forward = if menu_open { 0.0 } else { state.input.forward() };
            let strafe = if menu_open { 0.0 } else { state.input.strafe() };

            // Apply movement using current camera.yaw
            state.update_movement(dt);

            // Touch drag — user controls camera freely
            let (tdx, tdy) = camera::consume_touch_drag();
            if tdx != 0.0 || tdy != 0.0 {
                state.camera.apply_drag(tdx, tdy);
            }

            // Player faces camera direction when moving (standard 3rd-person).
            // Set AFTER touch drag so facing uses the latest camera.yaw.
            // Directly set visual_yaw for instant tracking while moving.
            if forward != 0.0 || strafe != 0.0 {
                state.local_move_yaw = state.camera.yaw;
                state.local_visual_yaw = state.camera.yaw;
            }

            // Camera terrain collision + distance smoothing
            state.update_camera(dt);

            // Visual yaw interpolation (only smooths idle→moving transitions)
            state.build_player_instances(now, dt);

            // Send input to server at ~20 Hz
            if now - state.last_send_time >= 50.0 {
                if let Some(conn) = &connection {
                    let yaw = state.camera.yaw;
                    let jumping = state.jump_sent;
                    let move_yaw = state.local_move_yaw;

                    conn.send_input(forward, strafe, yaw, jumping, move_yaw);
                    state.jump_sent = false;
                }
                state.last_send_time = now;
            }

            // Compute uniforms and render
            let (w, h) = state.renderer.surface_size();
            let aspect = w as f32 / h as f32;

            let (eye, look_target) = state.camera.eye_and_target();
            let view = glam::Mat4::look_at_rh(eye, look_target, glam::Vec3::Y);
            let proj = glam::Mat4::perspective_rh(
                game_core::camera::FOV,
                aspect,
                game_core::camera::NEAR_PLANE,
                game_core::camera::FAR_PLANE,
            );
            let view_proj = proj * view;
            let atmo = compute_atmosphere(state.sun_angle);

            let (cascade_vps, cascade_splits) = compute_cascade_view_projs(atmo.sun_dir, eye);

            let uniforms = Uniforms {
                view_proj: view_proj.to_cols_array(),
                camera_pos: eye.to_array(),
                _pad0: 0.0,
                sun_dir: atmo.sun_dir.to_array(),
                _pad1: 0.0,
                fog_color: atmo.fog_color,
                fog_density: atmo.fog_density,
                world_size: game_core::WORLD_SIZE,
                hm_res: game_core::HEIGHTMAP_RES as f32,
                fog_height_falloff: atmo.fog_height_falloff,
                time: state.time,
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

            state.renderer.update_uniforms(&uniforms);
            state.renderer.update_cascade_vps(&cascade_vps);
            state.renderer.render(eye, &view_proj, &state.players);

            // Chat bubbles: expire old, project active ones to screen
            state.active_bubbles.retain(|b| now - b.timestamp < 5000.0);
            if !state.active_bubbles.is_empty() {
                let vw = js_viewport_width();
                let vh = js_viewport_height();
                let tick_ms = (game_core::TICK_INTERVAL_SECS * 1000.0) as f64;
                let mut parts: Vec<String> = Vec::new();
                for bubble in &state.active_bubbles {
                    let pos = if Some(bubble.player_id) == state.local_player_id {
                        Some(state.local_pos)
                    } else {
                        state.remotes.get(&bubble.player_id).map(|rp| {
                            let t = ((now - rp.recv_time) / tick_ms).clamp(0.0, 1.0) as f32;
                            glam::Vec3::new(
                                rp.prev[0] + (rp.target[0] - rp.prev[0]) * t,
                                rp.prev[1] + (rp.target[1] - rp.prev[1]) * t,
                                rp.prev[2] + (rp.target[2] - rp.prev[2]) * t,
                            )
                        })
                    };
                    if let Some(pos) = pos {
                        let clip = view_proj * glam::Vec4::new(pos.x, pos.y + 1.7, pos.z, 1.0);
                        if clip.w > 0.0 {
                            let ndc_x = clip.x / clip.w;
                            let ndc_y = clip.y / clip.w;
                            if ndc_x > -2.0 && ndc_x < 2.0 && ndc_y > -2.0 && ndc_y < 2.0 {
                                let sx = (ndc_x + 1.0) * 0.5 * vw;
                                let sy = (1.0 - ndc_y) * 0.5 * vh;
                                let age = (now - bubble.timestamp) / 1000.0;
                                let escaped = json_escape(&bubble.text);
                                parts.push(format!(
                                    r#"{{"id":{},"x":{:.1},"y":{:.1},"text":"{}","age":{:.2}}}"#,
                                    bubble.player_id, sx, sy, escaped, age
                                ));
                            }
                        }
                    }
                }
                let json = format!("[{}]", parts.join(","));
                js_update_chat_bubbles(&json);
                state.had_active_bubbles = true;
            } else if state.had_active_bubbles {
                js_update_chat_bubbles("[]");
                state.had_active_bubbles = false;
            }
        }
        request_animation_frame(f.borrow().as_ref().unwrap());
    }));

    request_animation_frame(g.borrow().as_ref().unwrap());
}
