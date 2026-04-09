use serde::{Deserialize, Serialize};

pub type PlayerId = u16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    Input {
        forward: f32,
        strafe: f32,
        yaw: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome {
        your_id: PlayerId,
    },
    Snapshot {
        players: Vec<PlayerState>,
    },
    PlayerLeft {
        id: PlayerId,
    },
}
