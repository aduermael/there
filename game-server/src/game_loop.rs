use game_core::movement::{apply_movement, apply_vertical};
use game_core::protocol::{PlayerId, PlayerState, ServerMsg};
use game_core::terrain::{generate_heightmap, sample_height};
use game_core::{TICK_INTERVAL_SECS, TICK_RATE_HZ, WORLD_SIZE};
use glam::Vec3;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::room::{Player, RoomEvent};

pub async fn run(code: String, mut event_rx: mpsc::UnboundedReceiver<RoomEvent>) {
    log::info!("Room {} started", code);

    let heightmap = generate_heightmap();
    let mut players: HashMap<PlayerId, Player> = HashMap::new();
    let mut tick = interval(Duration::from_millis(1000 / TICK_RATE_HZ as u64));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                if players.is_empty() {
                    continue;
                }

                // Apply movement for each player
                for player in players.values_mut() {
                    let pos = Vec3::new(player.x, player.y, player.z);
                    let new_pos = apply_movement(
                        pos,
                        player.input_forward,
                        player.input_strafe,
                        player.input_yaw,
                        TICK_INTERVAL_SECS,
                        &heightmap,
                    );
                    player.x = new_pos.x;
                    player.z = new_pos.z;
                    player.yaw = player.input_yaw;

                    // Vertical physics
                    let terrain_y = sample_height(&heightmap, player.x, player.z);
                    let (new_y, new_vel) = apply_vertical(
                        player.y,
                        player.vertical_velocity,
                        terrain_y,
                        player.input_jump,
                        TICK_INTERVAL_SECS,
                    );
                    player.y = new_y;
                    player.vertical_velocity = new_vel;
                    player.input_jump = false;
                }

                // Build and broadcast snapshot
                let snapshot: Vec<PlayerState> = players
                    .values()
                    .map(|p| PlayerState {
                        id: p.id,
                        x: p.x,
                        y: p.y,
                        z: p.z,
                        yaw: p.yaw,
                    })
                    .collect();

                let msg = ServerMsg::Snapshot { players: snapshot };
                let mut disconnected = Vec::new();
                for player in players.values() {
                    if player.tx.send(msg.clone()).is_err() {
                        disconnected.push(player.id);
                    }
                }
                for id in disconnected {
                    players.remove(&id);
                    log::info!("Room {}: player {} disconnected (send failed)", code, id);
                }
            }

            event = event_rx.recv() => {
                match event {
                    Some(RoomEvent::Join { id, tx }) => {
                        // Spawn at center of world
                        let spawn_x = WORLD_SIZE / 2.0;
                        let spawn_z = WORLD_SIZE / 2.0;
                        let spawn_y = game_core::terrain::sample_height(&heightmap, spawn_x, spawn_z);

                        // Send welcome
                        let _ = tx.send(ServerMsg::Welcome { your_id: id });

                        players.insert(id, Player {
                            id,
                            x: spawn_x,
                            y: spawn_y,
                            z: spawn_z,
                            yaw: 0.0,
                            input_forward: 0.0,
                            input_strafe: 0.0,
                            input_yaw: 0.0,
                            input_jump: false,
                            vertical_velocity: 0.0,
                            tx,
                        });

                        log::info!("Room {}: player {} joined ({} total)", code, id, players.len());
                    }
                    Some(RoomEvent::Input { id, forward, strafe, yaw, jump }) => {
                        if let Some(player) = players.get_mut(&id) {
                            player.input_forward = forward;
                            player.input_strafe = strafe;
                            player.input_yaw = yaw;
                            if jump {
                                player.input_jump = true;
                            }
                        }
                    }
                    Some(RoomEvent::Leave { id }) => {
                        players.remove(&id);
                        log::info!("Room {}: player {} left ({} remaining)", code, id, players.len());

                        // Notify remaining players
                        let msg = ServerMsg::PlayerLeft { id };
                        for player in players.values() {
                            let _ = player.tx.send(msg.clone());
                        }
                    }
                    None => {
                        // Channel closed, room manager dropped us
                        log::info!("Room {} shutting down", code);
                        break;
                    }
                }
            }
        }
    }
}
