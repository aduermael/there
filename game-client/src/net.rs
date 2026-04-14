use std::cell::RefCell;
use std::rc::Rc;

use game_core::protocol::{ClientMsg, ServerMsg};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[derive(Clone)]
pub struct Connection {
    ws: Rc<RefCell<Option<web_sys::WebSocket>>>,
    url: String,
    incoming: Rc<RefCell<Vec<ServerMsg>>>,
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

        let ws_cell: Rc<RefCell<Option<web_sys::WebSocket>>> = Rc::new(RefCell::new(None));

        let conn = Self {
            ws: ws_cell.clone(),
            url: url.clone(),
            incoming: incoming.clone(),
        };

        conn.connect();

        // visibilitychange: reconnect when tab becomes visible again
        let ws_ref = ws_cell.clone();
        let url_ref = url.clone();
        let inc_ref = incoming;
        let on_visibility = Closure::wrap(Box::new(move |_: JsValue| {
            let document = web_sys::window().unwrap().document().unwrap();
            if !document.hidden() {
                // Check if ws is closed/closing, reconnect if needed
                let needs_reconnect = {
                    let ws = ws_ref.borrow();
                    match ws.as_ref() {
                        Some(ws) => ws.ready_state() > 1, // CLOSING or CLOSED
                        None => true,
                    }
                };
                if needs_reconnect {
                    log::info!("Tab visible — reconnecting");
                    Self::do_connect(&ws_ref, &url_ref, &inc_ref);
                }
            }
        }) as Box<dyn FnMut(_)>);
        let document = window.document().unwrap();
        document
            .add_event_listener_with_callback(
                "visibilitychange",
                on_visibility.as_ref().unchecked_ref(),
            )
            .unwrap();
        on_visibility.forget();

        conn
    }

    fn connect(&self) {
        Self::do_connect(&self.ws, &self.url, &self.incoming);
    }

    fn do_connect(
        ws_cell: &Rc<RefCell<Option<web_sys::WebSocket>>>,
        url: &str,
        incoming: &Rc<RefCell<Vec<ServerMsg>>>,
    ) {
        log::info!("Connecting to {url}");
        let ws = web_sys::WebSocket::new(url).expect("Failed to create WebSocket");
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // onopen
        let on_open = Closure::wrap(Box::new(move |_: JsValue| {
            log::info!("WebSocket connected");
        }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();

        // onmessage — deserialize ServerMsg and queue it
        let queue = incoming.clone();
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

        // onclose — reconnect with exponential backoff
        let ws_ref = ws_cell.clone();
        let url_owned = url.to_owned();
        let inc_ref = incoming.clone();
        let on_close = Closure::wrap(Box::new(move |_: JsValue| {
            log::info!("WebSocket closed — scheduling reconnect");
            let ws_ref2 = ws_ref.clone();
            let url2 = url_owned.clone();
            let inc2 = inc_ref.clone();
            // Reconnect after 1 second
            let cb = Closure::once(move || {
                // Only reconnect if still closed (not already reconnected by visibility handler)
                let needs = {
                    let ws = ws_ref2.borrow();
                    match ws.as_ref() {
                        Some(ws) => ws.ready_state() > 1,
                        None => true,
                    }
                };
                if needs {
                    Self::do_connect(&ws_ref2, &url2, &inc2);
                }
            });
            web_sys::window()
                .unwrap()
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    cb.as_ref().unchecked_ref(),
                    1000,
                )
                .unwrap();
            cb.forget();
        }) as Box<dyn FnMut(_)>);
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();

        *ws_cell.borrow_mut() = Some(ws);
    }

    pub fn send_chat(&self, text: &str) {
        let ws_borrow = self.ws.borrow();
        let ws = match ws_borrow.as_ref() {
            Some(ws) if ws.ready_state() == 1 => ws,
            _ => return,
        };
        let msg = ClientMsg::Chat { text: text.to_string() };
        if let Ok(bytes) = bincode::serialize(&msg) {
            let arr = js_sys::Uint8Array::from(bytes.as_slice());
            let _ = ws.send_with_array_buffer(&arr.buffer());
        }
    }

    pub fn send_input(&self, forward: f32, strafe: f32, yaw: f32, jumping: bool, move_yaw: f32) {
        let ws_borrow = self.ws.borrow();
        let ws = match ws_borrow.as_ref() {
            Some(ws) if ws.ready_state() == 1 => ws,
            _ => return,
        };
        let msg = ClientMsg::Input {
            forward,
            strafe,
            yaw,
            jumping,
            move_yaw,
        };
        if let Ok(bytes) = bincode::serialize(&msg) {
            let arr = js_sys::Uint8Array::from(bytes.as_slice());
            let _ = ws.send_with_array_buffer(&arr.buffer());
        }
    }
}
