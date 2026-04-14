/// Speed threshold to enter Walk from Idle.
pub const WALK_ENTER_SPEED: f32 = 0.5;
/// Speed threshold to exit Walk back to Idle.
pub const WALK_EXIT_SPEED: f32 = 0.15;

/// Movement states that drive animation selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimState {
    Idle,
    Walk,
    Run,
    Jump,
    Fall,
}

impl AnimState {
    /// Derive animation state from movement parameters.
    pub fn from_movement(speed: f32, vertical_velocity: f32) -> Self {
        if vertical_velocity > 1.0 {
            return AnimState::Jump;
        }
        if vertical_velocity < -1.0 {
            return AnimState::Fall;
        }
        if speed > 4.0 {
            return AnimState::Run;
        }
        if speed > WALK_ENTER_SPEED {
            return AnimState::Walk;
        }
        AnimState::Idle
    }

    /// Convert to u8 for network serialization.
    pub fn to_u8(self) -> u8 {
        match self {
            AnimState::Idle => 0,
            AnimState::Walk => 1,
            AnimState::Run => 2,
            AnimState::Jump => 3,
            AnimState::Fall => 4,
        }
    }

    /// Convert from u8 (network deserialization).
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => AnimState::Walk,
            2 => AnimState::Run,
            3 => AnimState::Jump,
            4 => AnimState::Fall,
            _ => AnimState::Idle,
        }
    }
}
