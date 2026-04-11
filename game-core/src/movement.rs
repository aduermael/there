use crate::{GRAVITY, JUMP_VELOCITY, MOVE_SPEED, WORLD_SIZE};
use crate::terrain::sample_height;
use glam::Vec3;

/// Apply movement to a position given input and dt.
/// Returns the new position, snapped to terrain height.
pub fn apply_movement(
    pos: Vec3,
    forward: f32,
    strafe: f32,
    yaw: f32,
    dt: f32,
    heightmap: &[f32],
) -> Vec3 {
    // Clamp input to [-1, 1]
    let forward = forward.clamp(-1.0, 1.0);
    let strafe = strafe.clamp(-1.0, 1.0);

    // Direction from yaw (yaw=0 faces -Z)
    let sin_yaw = yaw.sin();
    let cos_yaw = yaw.cos();

    let move_x = -sin_yaw * forward + cos_yaw * strafe;
    let move_z = -cos_yaw * forward - sin_yaw * strafe;

    // Normalize if magnitude > 1 (diagonal movement)
    let mag = (move_x * move_x + move_z * move_z).sqrt();
    let (move_x, move_z) = if mag > 1.0 {
        (move_x / mag, move_z / mag)
    } else {
        (move_x, move_z)
    };

    let mut new_x = pos.x + move_x * MOVE_SPEED * dt;
    let mut new_z = pos.z + move_z * MOVE_SPEED * dt;

    // Clamp to world bounds
    new_x = new_x.clamp(0.0, WORLD_SIZE - 0.01);
    new_z = new_z.clamp(0.0, WORLD_SIZE - 0.01);

    let new_y = sample_height(heightmap, new_x, new_z);

    Vec3::new(new_x, new_y, new_z)
}

/// Apply vertical physics (gravity + jump).
/// Returns (new_y, new_velocity).
pub fn apply_vertical(
    y: f32,
    velocity: f32,
    terrain_y: f32,
    jump_pressed: bool,
    dt: f32,
) -> (f32, f32) {
    let on_ground = y <= terrain_y + 0.01;

    let mut vel = velocity;

    // Initiate jump when on ground and jump pressed
    if on_ground && jump_pressed {
        vel = JUMP_VELOCITY;
    }

    // Apply gravity when airborne
    if !on_ground || vel > 0.0 {
        vel += GRAVITY * dt;
    }

    let mut new_y = y + vel * dt;

    // Land on terrain
    if new_y <= terrain_y {
        new_y = terrain_y;
        vel = 0.0;
    }

    (new_y, vel)
}
