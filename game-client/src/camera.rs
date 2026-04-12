use std::cell::Cell;

use glam::{Mat4, Vec3};
use wasm_bindgen::prelude::*;

const SENSITIVITY: f32 = 0.005;
const ZOOM_SPEED: f32 = 0.1;
const MIN_PITCH: f32 = 0.05;
const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.05;
const MIN_DISTANCE: f32 = 5.0;
const MAX_DISTANCE: f32 = 200.0;
// Accumulated touch drag deltas from the camera-control web component.
thread_local! {
    static TOUCH_DRAG: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
}

/// Called from JS camera-control web component.
#[wasm_bindgen]
pub fn on_camera_drag(dx: f32, dy: f32) {
    TOUCH_DRAG.with(|c| {
        let (cx, cy) = c.get();
        c.set((cx + dx, cy + dy));
    });
}

/// Drain accumulated touch drag deltas (called once per frame).
pub fn consume_touch_drag() -> (f32, f32) {
    TOUCH_DRAG.with(|c| {
        let val = c.get();
        c.set((0.0, 0.0));
        val
    })
}

pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    dragging: bool,
    last_x: f32,
    last_y: f32,
}

impl OrbitCamera {
    pub fn new(target: Vec3, yaw: f32, pitch: f32, distance: f32) -> Self {
        Self {
            target,
            yaw,
            pitch: pitch.clamp(MIN_PITCH, MAX_PITCH),
            distance: distance.clamp(MIN_DISTANCE, MAX_DISTANCE),
            dragging: false,
            last_x: 0.0,
            last_y: 0.0,
        }
    }

    pub fn on_pointer_down(&mut self, x: f32, y: f32) {
        self.dragging = true;
        self.last_x = x;
        self.last_y = y;
    }

    pub fn on_pointer_move(&mut self, x: f32, y: f32) {
        if !self.dragging {
            return;
        }
        let dx = x - self.last_x;
        let dy = y - self.last_y;
        self.last_x = x;
        self.last_y = y;

        self.yaw -= dx * SENSITIVITY;
        self.pitch = (self.pitch + dy * SENSITIVITY).clamp(MIN_PITCH, MAX_PITCH);
    }

    pub fn on_pointer_up(&mut self) {
        self.dragging = false;
    }

    pub fn on_wheel(&mut self, delta_y: f32) {
        self.distance = (self.distance + delta_y * ZOOM_SPEED).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Apply drag deltas directly (used by touch camera control).
    pub fn apply_drag(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * SENSITIVITY;
        self.pitch = (self.pitch + dy * SENSITIVITY).clamp(MIN_PITCH, MAX_PITCH);
    }

    /// Camera position in world space (spherical → cartesian).
    fn raw_eye(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Eye position with ray-based terrain collision.
    /// Casts a ray from target toward raw_eye, sampling terrain along the way.
    /// Pulls camera closer if any sample point is below terrain + clearance.
    pub fn eye(&self, heightmap: &[f32]) -> Vec3 {
        let raw = self.raw_eye();
        let dir = raw - self.target;
        let full_dist = dir.length();
        if full_dist < 0.001 {
            return raw;
        }

        const CLEARANCE: f32 = 1.8;
        const RAY_STEPS: u32 = 16;

        let mut safe_t = 0.0_f32;
        for i in 1..=RAY_STEPS {
            let t = i as f32 / RAY_STEPS as f32;
            let p = self.target + dir * t;
            let terrain_y = game_core::terrain::sample_height(heightmap, p.x, p.z);
            if p.y < terrain_y + CLEARANCE {
                // This sample is underground — use the previous safe t
                break;
            }
            safe_t = t;
        }

        if safe_t >= 1.0 - 1e-5 {
            // Full distance is clear
            raw
        } else {
            // Pull camera to last safe point along the ray
            self.target + dir * safe_t
        }
    }

}
