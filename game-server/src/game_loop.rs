use game_core::movement::{apply_movement, apply_vertical};
use game_core::protocol::{PlayerId, PlayerState, ServerMsg};
use game_core::terrain::{generate_heightmap, sample_height};
use game_core::{AnimState, TICK_INTERVAL_SECS, TICK_RATE_HZ};
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
                    let airborne = player.vertical_velocity != 0.0;
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

                    // Smoothly rotate toward move_yaw when moving; retain facing when idle
                    if player.input_forward != 0.0 || player.input_strafe != 0.0 {
                        let turn_speed = 12.0; // radians/sec
                        let max_step = turn_speed * TICK_INTERVAL_SECS;
                        player.yaw = game_core::movement::lerp_angle(player.yaw, player.input_move_yaw, max_step);
                    }

                    // Vertical physics — only when jumping or airborne
                    let terrain_y = sample_height(&heightmap, player.x, player.z);
                    if player.input_jump || airborne {
                        let (new_y, new_vel) = apply_vertical(
                            player.y,
                            player.vertical_velocity,
                            terrain_y,
                            player.input_jump,
                            TICK_INTERVAL_SECS,
                        );
                        player.y = new_y;
                        player.vertical_velocity = new_vel;
                    } else {
                        // Grounded: use terrain height from apply_movement
                        player.y = new_pos.y;
                    }
                    player.input_jump = false;

                    // Derive animation state from movement
                    let dx = player.x - pos.x;
                    let dz = player.z - pos.z;
                    let horiz_speed = (dx * dx + dz * dz).sqrt() / TICK_INTERVAL_SECS;
                    player.anim_state = AnimState::from_movement(horiz_speed, player.vertical_velocity).to_u8();
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
                        anim_state: p.anim_state,
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
                        let (spawn_x, spawn_z) = game_core::terrain::find_clear_spawn(&heightmap);
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
                            input_move_yaw: 0.0,
                            input_jump: false,
                            vertical_velocity: 0.0,
                            anim_state: 0,
                            name: format!("Player {id}"),
                            tx,
                        });

                        // Broadcast name update with all player names (includes new player)
                        let names: Vec<(PlayerId, String)> = players.values()
                            .map(|p| (p.id, p.name.clone()))
                            .collect();
                        let name_msg = ServerMsg::NameUpdate { names };
                        for player in players.values() {
                            let _ = player.tx.send(name_msg.clone());
                        }

                        log::info!("Room {}: player {} joined ({} total)", code, id, players.len());
                    }
                    Some(RoomEvent::Input { id, forward, strafe, yaw, jump, move_yaw }) => {
                        if let Some(player) = players.get_mut(&id) {
                            player.input_forward = forward;
                            player.input_strafe = strafe;
                            player.input_yaw = yaw;
                            player.input_move_yaw = move_yaw;
                            if jump {
                                player.input_jump = true;
                            }
                        }
                    }
                    Some(RoomEvent::Chat { id, text }) => {
                        let msg = ServerMsg::Chat { from: id, text };
                        for player in players.values() {
                            let _ = player.tx.send(msg.clone());
                        }
                    }
                    Some(RoomEvent::SetName { id, name }) => {
                        if let Some(player) = players.get_mut(&id) {
                            player.name = name;
                        }
                        let names: Vec<(PlayerId, String)> = players.values()
                            .map(|p| (p.id, p.name.clone()))
                            .collect();
                        let msg = ServerMsg::NameUpdate { names };
                        for player in players.values() {
                            let _ = player.tx.send(msg.clone());
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
