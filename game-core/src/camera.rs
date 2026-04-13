use glam::Vec3;

/// Upward offset from player feet to orbit center (~shoulder height).
pub const TARGET_Y_OFFSET: f32 = 1.4;
pub const MIN_PITCH: f32 = 0.05;
pub const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.05;
pub const MIN_DISTANCE: f32 = 2.0;
pub const MAX_DISTANCE: f32 = 12.0;
pub const DEFAULT_PITCH: f32 = 0.35;
pub const DEFAULT_DISTANCE: f32 = 6.0;
pub const FOV: f32 = std::f32::consts::FRAC_PI_4;
pub const NEAR_PLANE: f32 = 0.1;
pub const FAR_PLANE: f32 = 500.0;

/// Compute orbit camera eye position and look target from spherical parameters.
///
/// `target` is the player foot position. Returns `(eye_position, look_target)`.
pub fn orbit_eye(target: Vec3, yaw: f32, pitch: f32, distance: f32) -> (Vec3, Vec3) {
    let center = target + Vec3::new(0.0, TARGET_Y_OFFSET, 0.0);
    let x = distance * pitch.cos() * yaw.sin();
    let y = distance * pitch.sin();
    let z = distance * pitch.cos() * yaw.cos();
    (center + Vec3::new(x, y, z), center)
}
