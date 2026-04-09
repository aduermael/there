use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

mod camera;
mod input;
mod net;
mod player;
mod renderer;
mod terrain;

use camera::OrbitCamera;
use game_core::protocol::ServerMsg;
use input::InputState;
use net::Connection;
use player::PlayerInstance;
use renderer::Renderer;
use terrain::Uniforms;

struct GameState {
    renderer: Renderer,
    camera: OrbitCamera,
    heightmap_data: Vec<f32>,
    players: Vec<PlayerInstance>,
    input: InputState,
    last_send_time: f64,
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

    let camera = OrbitCamera::new(
        glam::Vec3::new(128.0, 15.0, 128.0), // target: world center
        0.5,                                   // yaw
        0.35,                                  // pitch
        80.0,                                  // distance
    );

    // Test player at world center
    let spawn_y = game_core::terrain::sample_height(&heightmap_data, 128.0, 128.0);
    let test_player = PlayerInstance {
        pos_yaw: [128.0, spawn_y, 128.0, 0.0],
        color: [0.90, 0.30, 0.25, 0.0],
    };

    // WebSocket connection — extract room code from URL path
    let incoming: Rc<RefCell<Vec<ServerMsg>>> = Rc::new(RefCell::new(Vec::new()));
    let pathname = window.location().pathname().unwrap_or_default();
    let room_code = pathname.trim_start_matches('/');
    let connection = if !room_code.is_empty() {
        log::info!("Room code: {room_code}");
        Some(Connection::new(room_code, incoming.clone()))
    } else {
        log::info!("No room code — offline mode");
        None
    };

    let state = Rc::new(RefCell::new(GameState {
        renderer,
        camera,
        heightmap_data,
        players: vec![test_player],
        input: InputState::new(),
        last_send_time: 0.0,
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

    // Pointer down
    let s = state.clone();
    let on_down = Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
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
    _incoming: Rc<RefCell<Vec<ServerMsg>>>,
) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::new(move || {
        {
            let mut state = state.borrow_mut();

            // Send input to server at ~20 Hz
            let now = js_sys::Date::now();
            if now - state.last_send_time >= 50.0 {
                if let Some(conn) = &connection {
                    conn.send_input(
                        state.input.forward(),
                        state.input.strafe(),
                        state.camera.yaw,
                    );
                }
                state.last_send_time = now;
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
