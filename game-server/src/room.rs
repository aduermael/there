use game_core::protocol::{PlayerId, ServerMsg};
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::game_loop;

pub struct Player {
    pub id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub input_forward: f32,
    pub input_strafe: f32,
    pub input_yaw: f32,
    pub input_move_yaw: f32,
    pub input_jump: bool,
    pub vertical_velocity: f32,
    pub anim_state: u8,
    pub tx: mpsc::UnboundedSender<ServerMsg>,
}

pub enum RoomEvent {
    Join {
        id: PlayerId,
        tx: mpsc::UnboundedSender<ServerMsg>,
    },
    Input {
        id: PlayerId,
        forward: f32,
        strafe: f32,
        yaw: f32,
        jump: bool,
        move_yaw: f32,
    },
    Leave {
        id: PlayerId,
    },
}

struct Room {
    event_tx: mpsc::UnboundedSender<RoomEvent>,
    player_count: usize,
}

pub struct RoomManager {
    rooms: HashMap<String, Room>,
    next_player_id: PlayerId,
}

impl RoomManager {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
            next_player_id: 1,
        }
    }

    pub fn generate_code(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
        let mut code = String::with_capacity(4);
        let mut n = seed;
        for _ in 0..4 {
            code.push(chars[(n % chars.len() as u128) as usize]);
            n /= chars.len() as u128;
        }
        // Ensure uniqueness
        if self.rooms.contains_key(&code) {
            // Add some extra entropy
            let extra = seed.wrapping_mul(6364136223846793005);
            code.clear();
            let mut n = extra;
            for _ in 0..4 {
                code.push(chars[(n % chars.len() as u128) as usize]);
                n /= chars.len() as u128;
            }
        }
        code
    }

    pub fn join_or_create(&mut self, code: &str) -> (PlayerId, mpsc::UnboundedSender<RoomEvent>) {
        let id = self.next_player_id;
        self.next_player_id = self.next_player_id.wrapping_add(1);

        let room = self.rooms.entry(code.to_string()).or_insert_with(|| {
            let (event_tx, event_rx) = mpsc::unbounded_channel();
            // Spawn the room's game loop
            tokio::spawn(game_loop::run(code.to_string(), event_rx));
            Room {
                event_tx,
                player_count: 0,
            }
        });

        room.player_count += 1;
        (id, room.event_tx.clone())
    }

    pub fn list_rooms(&self) -> Vec<(String, usize)> {
        self.rooms
            .iter()
            .map(|(code, room)| (code.clone(), room.player_count))
            .collect()
    }

    pub fn player_left(&mut self, code: &str) {
        if let Some(room) = self.rooms.get_mut(code) {
            room.player_count = room.player_count.saturating_sub(1);
            if room.player_count == 0 {
                // Room will shut down when event channel is dropped
                self.rooms.remove(code);
            }
        }
    }
}
