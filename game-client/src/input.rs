use std::cell::Cell;
use wasm_bindgen::prelude::*;

thread_local! {
    static JOY_INPUT: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
    static JUMP_PRESSED: Cell<bool> = const { Cell::new(false) };
}

/// Called from JS virtual-joystick web component.
#[wasm_bindgen]
pub fn set_joystick_input(forward: f32, strafe: f32) {
    JOY_INPUT.with(|j| j.set((forward, strafe)));
}

pub fn joystick_input() -> (f32, f32) {
    JOY_INPUT.with(|j| j.get())
}

/// Called from JS jump button web component.
#[wasm_bindgen]
pub fn on_jump_pressed() {
    JUMP_PRESSED.with(|j| j.set(true));
}

/// Consume the latched jump flag (returns true once, then resets).
pub fn consume_jump() -> bool {
    JUMP_PRESSED.with(|j| {
        let v = j.get();
        j.set(false);
        v
    })
}

pub struct InputState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    space: bool,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            up: false,
            down: false,
            left: false,
            right: false,
            space: false,
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

    pub fn jump_pressed(&self) -> bool {
        self.space || consume_jump()
    }

    /// Returns true if the key was a game input key (caller should prevent default).
    pub fn on_key_down(&mut self, code: &str) -> bool {
        match code {
            "KeyW" | "ArrowUp" => self.up = true,
            "KeyS" | "ArrowDown" => self.down = true,
            "KeyA" | "ArrowLeft" => self.left = true,
            "KeyD" | "ArrowRight" => self.right = true,
            "Space" => self.space = true,
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
            "Space" => self.space = false,
            _ => {}
        }
    }
}
