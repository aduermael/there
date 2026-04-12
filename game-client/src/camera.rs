use std::cell::Cell;

use glam::Vec3;
use wasm_bindgen::prelude::*;

const SENSITIVITY: f32 = 0.005;
const ZOOM_SPEED: f32 = 0.02;
const MIN_PITCH: f32 = 0.05;
const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.05;
const MIN_DISTANCE: f32 = 3.0;
const MAX_DISTANCE: f32 = 20.0;
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

/// Smoothing rate when terrain forces camera closer (fast snap-in).
const APPROACH_RATE: f32 = 10.0;
/// Smoothing rate when camera recovers to desired distance (slow ease-out).
const RECOVER_RATE: f32 = 3.0;

pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// User-desired distance (set by scroll wheel).
    desired_distance: f32,
    /// Smoothed effective distance (tracks collision-limited distance).
    effective_distance: f32,
    dragging: bool,
    last_x: f32,
    last_y: f32,
}

impl OrbitCamera {
    pub fn new(target: Vec3, yaw: f32, pitch: f32, distance: f32) -> Self {
        let d = distance.clamp(MIN_DISTANCE, MAX_DISTANCE);
        Self {
            target,
            yaw,
            pitch: pitch.clamp(MIN_PITCH, MAX_PITCH),
            desired_distance: d,
            effective_distance: d,
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
        self.desired_distance = (self.desired_distance + delta_y * ZOOM_SPEED).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Apply drag deltas directly (used by touch camera control).
    pub fn apply_drag(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * SENSITIVITY;
        self.pitch = (self.pitch + dy * SENSITIVITY).clamp(MIN_PITCH, MAX_PITCH);
    }

    /// Camera position in world space (spherical → cartesian) at a given distance.
    fn eye_at(&self, dist: f32) -> Vec3 {
        let x = dist * self.pitch.cos() * self.yaw.sin();
        let y = dist * self.pitch.sin();
        let z = dist * self.pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Update camera each frame: raycast terrain collision + smooth distance.
    pub fn update(&mut self, dt: f32, heightmap: &[f32]) {
        // Raycast from target toward desired eye to find max safe distance
        let raw = self.eye_at(self.desired_distance);
        let dir = raw - self.target;
        let full_dist = dir.length();

        let collision_dist = if full_dist < 0.001 {
            self.desired_distance
        } else {
            const CLEARANCE: f32 = 1.8;
            const RAY_STEPS: u32 = 16;

            let mut safe_t = 0.0_f32;
            for i in 1..=RAY_STEPS {
                let t = i as f32 / RAY_STEPS as f32;
                let p = self.target + dir * t;
                let terrain_y = game_core::terrain::sample_height(heightmap, p.x, p.z);
                if p.y < terrain_y + CLEARANCE {
                    break;
                }
                safe_t = t;
            }
            safe_t * self.desired_distance
        };

        // Smooth: fast approach, slow recovery
        let rate = if collision_dist < self.effective_distance {
            APPROACH_RATE
        } else {
            RECOVER_RATE
        };
        let alpha = 1.0 - (-rate * dt).exp();
        self.effective_distance += (collision_dist - self.effective_distance) * alpha;
    }

    /// Current eye position (call after update).
    pub fn eye(&self) -> Vec3 {
        self.eye_at(self.effective_distance)
    }

}
