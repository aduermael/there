use std::cell::Cell;
use wasm_bindgen::prelude::*;

thread_local! {
    static JOY_INPUT: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
}

/// Called from JS virtual-joystick web component.
#[wasm_bindgen]
pub fn set_joystick_input(forward: f32, strafe: f32) {
    JOY_INPUT.with(|j| j.set((forward, strafe)));
}

pub fn joystick_input() -> (f32, f32) {
    JOY_INPUT.with(|j| j.get())
}

pub struct InputState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            up: false,
            down: false,
            left: false,
            right: false,
        }
    }

    pub fn forward(&self) -> f32 {
        let kbd = (self.up as i32 - self.down as i32) as f32;
        let (joy_fwd, _) = joystick_input();
        (kbd + joy_fwd).clamp(-1.0, 1.0)
    }

    pub fn strafe(&self) -> f32 {
        let kbd = (self.right as i32 - self.left as i32) as f32;
        let (_, joy_str) = joystick_input();
        (kbd + joy_str).clamp(-1.0, 1.0)
    }

    /// Returns true if the key was a game input key (caller should prevent default).
    pub fn on_key_down(&mut self, code: &str) -> bool {
        match code {
            "KeyW" | "ArrowUp" => self.up = true,
            "KeyS" | "ArrowDown" => self.down = true,
            "KeyA" | "ArrowLeft" => self.left = true,
            "KeyD" | "ArrowRight" => self.right = true,
            _ => return false,
        }
        true
    }

    pub fn on_key_up(&mut self, code: &str) {
        match code {
            "KeyW" | "ArrowUp" => self.up = false,
            "KeyS" | "ArrowDown" => self.down = false,
            "KeyA" | "ArrowLeft" => self.left = false,
            "KeyD" | "ArrowRight" => self.right = false,
            _ => {}
        }
    }
}
