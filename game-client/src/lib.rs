use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

mod renderer;
mod terrain;

use renderer::Renderer;
use terrain::Uniforms;

struct GameState {
    renderer: Renderer,
    #[allow(dead_code)]
    heightmap_data: Vec<f32>,
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

    let renderer = Renderer::new(canvas, &heightmap_data).await;

    let state = Rc::new(RefCell::new(GameState {
        renderer,
        heightmap_data,
    }));
    start_render_loop(state);
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("requestAnimationFrame failed");
}

fn start_render_loop(state: Rc<RefCell<GameState>>) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::new(move || {
        {
            let state = state.borrow();
            let (w, h) = state.renderer.surface_size();
            let aspect = w as f32 / h as f32;

            // Fixed camera for now (orbit camera added in 3e)
            let eye = glam::Vec3::new(128.0, 60.0, 200.0);
            let target = glam::Vec3::new(128.0, 15.0, 128.0);
            let view = glam::Mat4::look_at_rh(eye, target, glam::Vec3::Y);
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
            state.renderer.render();
        }
        request_animation_frame(f.borrow().as_ref().unwrap());
    }));

    request_animation_frame(g.borrow().as_ref().unwrap());
}
