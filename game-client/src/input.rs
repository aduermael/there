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
        (self.up as i32 - self.down as i32) as f32
    }

    pub fn strafe(&self) -> f32 {
        (self.right as i32 - self.left as i32) as f32
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
