use std::cell::RefCell;
use std::rc::Rc;

use game_core::protocol::{ClientMsg, ServerMsg};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

pub struct Connection {
    ws: web_sys::WebSocket,
}

impl Connection {
    pub fn new(room_code: &str, incoming: Rc<RefCell<Vec<ServerMsg>>>) -> Self {
        let window = web_sys::window().unwrap();
        let location = window.location();
        let protocol = if location.protocol().unwrap_or_default() == "https:" {
            "wss"
        } else {
            "ws"
        };
        let host = location.host().unwrap_or_default();
        let url = format!("{protocol}://{host}/ws?room={room_code}");

        log::info!("Connecting to {url}");
        let ws = web_sys::WebSocket::new(&url).expect("Failed to create WebSocket");
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // onopen
        let on_open = Closure::wrap(Box::new(move |_: JsValue| {
            log::info!("WebSocket connected");
        }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();

        // onmessage — deserialize ServerMsg and queue it
        let queue = incoming;
        let on_message = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
            if let Ok(buffer) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                match bincode::deserialize::<ServerMsg>(&bytes) {
                    Ok(msg) => queue.borrow_mut().push(msg),
                    Err(e) => log::warn!("Failed to deserialize ServerMsg: {e}"),
                }
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        // onerror
        let on_error = Closure::wrap(Box::new(move |_: JsValue| {
            log::error!("WebSocket error");
        }) as Box<dyn FnMut(_)>);
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();

        // onclose
        let on_close = Closure::wrap(Box::new(move |_: JsValue| {
            log::info!("WebSocket closed");
        }) as Box<dyn FnMut(_)>);
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();

        Self { ws }
    }

    pub fn send_input(&self, forward: f32, strafe: f32, yaw: f32) {
        if self.ws.ready_state() != 1 {
            return; // not OPEN
        }
        let msg = ClientMsg::Input {
            forward,
            strafe,
            yaw,
        };
        if let Ok(bytes) = bincode::serialize(&msg) {
            let arr = js_sys::Uint8Array::from(bytes.as_slice());
            let _ = self.ws.send_with_array_buffer(&arr.buffer());
        }
    }
}
