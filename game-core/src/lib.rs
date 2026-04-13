pub mod camera;
pub mod protocol;
pub mod movement;
pub mod terrain;

// Constants
pub const TICK_RATE_HZ: u32 = 20;
pub const TICK_INTERVAL_SECS: f32 = 1.0 / TICK_RATE_HZ as f32;
pub const MOVE_SPEED: f32 = 5.0;
pub const WORLD_SIZE: f32 = 256.0;
pub const HEIGHTMAP_RES: u32 = 512;
pub const DAYLIGHT_CYCLE_SECS: f32 = 120.0;
pub const GRAVITY: f32 = -20.0;
pub const JUMP_VELOCITY: f32 = 8.0;
pub const WATER_LEVEL: f32 = 8.0;
pub const PLAYER_TURN_SPEED: f32 = 12.0;
