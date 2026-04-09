pub mod protocol;
pub mod movement;
pub mod terrain;

// Constants
pub const TICK_RATE_HZ: u32 = 20;
pub const TICK_INTERVAL_SECS: f32 = 1.0 / TICK_RATE_HZ as f32;
pub const MOVE_SPEED: f32 = 5.0;
pub const WORLD_SIZE: f32 = 256.0;
pub const HEIGHTMAP_RES: u32 = 512;
