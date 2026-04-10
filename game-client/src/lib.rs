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
use game_render::{player_color, PlayerInstance, Uniforms};
use input::InputState;
use net::Connection;
use renderer::Renderer;

#[wasm_bindgen(inline_js = "
export function hud_set_room(code) {
    const el = document.querySelector('game-hud');
    if (el) el.roomCode = code;
}
export function hud_set_players(n) {
    const el = document.querySelector('game-hud');
    if (el) el.playerCount = n;
}
")]
extern "C" {
    fn hud_set_room(code: &str);
    fn hud_set_players(n: u32);
}

struct RemotePlayer {
    prev: [f32; 4],   // [x, y, z, yaw] from previous snapshot
    target: [f32; 4], // [x, y, z, yaw] from latest snapshot
    recv_time: f64,    // ms timestamp when target was received
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

    // Spawn at world center
    let spawn_y = game_core::terrain::sample_height(&heightmap_data, 128.0, 128.0);
    let local_pos = glam::Vec3::new(128.0, spawn_y, 128.0);

    let camera = OrbitCamera::new(
        local_pos,
        0.5,  // yaw
        0.35, // pitch
        15.0, // distance
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
        hud_set_room(room_code);
        Some(Connection::new(room_code, incoming.clone()))
    } else {
        log::info!("No room code — offline mode");
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
    }));

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
            let dt = dt.clamp(0.0, 0.1); // cap at 100ms to avoid jumps
            state.last_frame_time = now;

            // Process incoming server messages
            let messages: Vec<ServerMsg> = incoming.borrow_mut().drain(..).collect();
            for msg in messages {
                match msg {
                    ServerMsg::Welcome { your_id } => {
                        log::info!("Assigned player ID: {your_id}");
                        state.local_player_id = Some(your_id);
                        state.remotes.clear(); // fresh session
                        let c = player_color(your_id);
                        if let Some(p) = state.players.first_mut() {
                            p.color = [c[0], c[1], c[2], 0.0];
                        }
                    }
                    ServerMsg::Snapshot { players } => {
                        hud_set_players(players.len() as u32);
                        let local_id = state.local_player_id;

                        // Track which remote IDs are in this snapshot
                        let mut seen = std::collections::HashSet::new();

                        for ps in &players {
                            if Some(ps.id) == local_id {
                                // Snap-correction: compare server position to predicted
                                let server_pos =
                                    glam::Vec3::new(ps.x, ps.y, ps.z);
                                let delta = server_pos - state.local_pos;
                                let dist = delta.length();
                                if dist > 5.0 {
                                    // Large mismatch — snap directly
                                    state.local_pos = server_pos;
                                } else if dist > 0.1 {
                                    // Small mismatch — blend toward server
                                    state.local_pos += delta * 0.3;
                                }
                                continue;
                            }
                            seen.insert(ps.id);
                            let new_pos = [ps.x, ps.y, ps.z, ps.yaw];
                            if let Some(rp) = state.remotes.get_mut(&ps.id) {
                                // Shift target → prev, set new target
                                rp.prev = rp.target;
                                rp.target = new_pos;
                                rp.recv_time = now;
                            } else {
                                // New remote player — no interpolation on first frame
                                state.remotes.insert(ps.id, RemotePlayer {
                                    prev: new_pos,
                                    target: new_pos,
                                    recv_time: now,
                                });
                            }
                        }

                        // Remove remotes no longer in snapshot
                        state.remotes.retain(|id, _| seen.contains(id));
                    }
                    ServerMsg::PlayerLeft { id } => {
                        log::info!("Player {id} left");
                        state.remotes.remove(&id);
                    }
                }
            }

            // Local player movement (client prediction)
            let forward = state.input.forward();
            let strafe = state.input.strafe();
            let yaw = state.camera.yaw;
            if forward != 0.0 || strafe != 0.0 {
                state.local_pos = game_core::movement::apply_movement(
                    state.local_pos,
                    forward,
                    strafe,
                    yaw,
                    dt,
                    &state.heightmap_data,
                );
            }

            // Update camera to follow player
            state.camera.target = state.local_pos;

            // Rebuild players list: local player + interpolated remotes
            state.players.clear();
            let local_color = state
                .local_player_id
                .map(|id| player_color(id))
                .unwrap_or([0.90, 0.30, 0.25]);
            let pos = state.local_pos;
            state.players.push(PlayerInstance {
                pos_yaw: [pos.x, pos.y, pos.z, yaw],
                color: [local_color[0], local_color[1], local_color[2], 0.0],
            });

            // Interpolate remote players between prev and target
            let tick_ms = (game_core::TICK_INTERVAL_SECS * 1000.0) as f64;
            let remote_instances: Vec<PlayerInstance> = state
                .remotes
                .iter()
                .map(|(&id, rp)| {
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
                    let c = player_color(id);
                    PlayerInstance {
                        pos_yaw: [x, y, z, interp_yaw],
                        color: [c[0], c[1], c[2], 0.0],
                    }
                })
                .collect();
            state.players.extend(remote_instances);

            // Send input to server at ~20 Hz
            if now - state.last_send_time >= 50.0 {
                if let Some(conn) = &connection {
                    conn.send_input(forward, strafe, yaw);
                }
                state.last_send_time = now;
            }

            // Apply touch camera drag (from camera-control web component)
            let (tdx, tdy) = camera::consume_touch_drag();
            if tdx != 0.0 || tdy != 0.0 {
                state.camera.apply_drag(tdx, tdy);
            }

            let (w, h) = state.renderer.surface_size();
            let aspect = w as f32 / h as f32;

            let eye = state.camera.eye(&state.heightmap_data);
            let view = glam::Mat4::look_at_rh(eye, state.camera.target, glam::Vec3::Y);
            let proj = glam::Mat4::perspective_rh(
                std::f32::consts::FRAC_PI_4,
                aspect,
                0.1,
                500.0,
            );
            let view_proj = proj * view;
            let sun_dir = glam::Vec3::new(0.5, 0.8, 0.3).normalize();

            let uniforms = Uniforms {
                view_proj: view_proj.to_cols_array(),
                camera_pos: eye.to_array(),
                _pad0: 0.0,
                sun_dir: sun_dir.to_array(),
                _pad1: 0.0,
                fog_color: [0.53, 0.81, 0.92],
                fog_far: 300.0,
                world_size: game_core::WORLD_SIZE,
                hm_res: game_core::HEIGHTMAP_RES as f32,
                _pad2: [0.0; 2],
            };

            state.renderer.update_uniforms(&uniforms);
            state.renderer.render(eye, &view_proj, &state.players);
        }
        request_animation_frame(f.borrow().as_ref().unwrap());
    }));

    request_animation_frame(g.borrow().as_ref().unwrap());
}
