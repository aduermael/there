use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

mod renderer;

use renderer::Renderer;

struct GameState {
    renderer: Renderer,
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

    let renderer = Renderer::new(canvas).await;
    log::info!("Renderer initialized");

    let state = Rc::new(RefCell::new(GameState { renderer }));
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
            state.renderer.render();
        }
        request_animation_frame(f.borrow().as_ref().unwrap());
    }));

    request_animation_frame(g.borrow().as_ref().unwrap());
}
