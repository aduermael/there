mod game_loop;
mod room;

use axum::extract::ws::WebSocket;
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Json;
use axum::Router;
use room::{RoomEvent, RoomManager};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

type SharedRoomManager = Arc<RwLock<RoomManager>>;

#[derive(serde::Deserialize)]
struct WsQuery {
    room: String,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let rooms: SharedRoomManager = Arc::new(RwLock::new(RoomManager::new()));

    // Static files served from web/, room code paths fall back to index.html
    let serve = ServeDir::new("web").fallback(ServeFile::new("web/index.html"));

    let app = Router::new()
        .route("/ws", get(handle_ws))
        .route("/api/rooms", get(handle_list_rooms))
        .route("/api/rooms/new", get(handle_new_room))
        .fallback_service(serve)
        .layer(SetResponseHeaderLayer::overriding(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        ))
        .with_state(rooms);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(21617);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!("Server listening on http://localhost:{}", addr.port());

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
        log::error!("Failed to bind to {}: {} (port already in use?)", addr, e);
        std::process::exit(1);
    });
    axum::serve(listener, app).await.unwrap();
}

async fn handle_new_room(State(rooms): State<SharedRoomManager>) -> Json<serde_json::Value> {
    let code = rooms.read().await.generate_code();
    Json(serde_json::json!({ "code": code }))
}

async fn handle_list_rooms(State(rooms): State<SharedRoomManager>) -> Json<serde_json::Value> {
    let list = rooms.read().await.list_rooms();
    let arr: Vec<serde_json::Value> = list
        .into_iter()
        .map(|(code, count)| {
            serde_json::json!({ "code": code, "player_count": count })
        })
        .collect();
    Json(serde_json::Value::Array(arr))
}

async fn handle_ws(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(rooms): State<SharedRoomManager>,
) -> impl IntoResponse {
    let room_code = query.room.to_uppercase();
    ws.on_upgrade(move |socket| handle_socket(socket, room_code, rooms))
}

async fn handle_socket(socket: WebSocket, room_code: String, rooms: SharedRoomManager) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Join room
    let (player_id, event_tx) = {
        let mut mgr = rooms.write().await;
        mgr.join_or_create(&room_code)
    };

    // Channel for server → this player
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::unbounded_channel();

    // Notify room of new player
    let _ = event_tx.send(RoomEvent::Join {
        id: player_id,
        tx: msg_tx,
    });

    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};

    // Task: forward server messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            let bytes = bincode::serialize(&msg).unwrap();
            if ws_tx.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    });

    // Task: forward WebSocket messages to room
    let event_tx_clone = event_tx.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Binary(data) => {
                    if let Ok(client_msg) =
                        bincode::deserialize::<game_core::protocol::ClientMsg>(&data)
                    {
                        match client_msg {
                            game_core::protocol::ClientMsg::Input {
                                forward,
                                strafe,
                                yaw,
                                jumping,
                                move_yaw,
                            } => {
                                let _ = event_tx_clone.send(RoomEvent::Input {
                                    id: player_id,
                                    forward,
                                    strafe,
                                    yaw,
                                    jump: jumping,
                                    move_yaw,
                                });
                            }
                            game_core::protocol::ClientMsg::Chat { text } => {
                                let text = text.trim().to_string();
                                if !text.is_empty() && text.len() <= 200 {
                                    let _ = event_tx_clone.send(RoomEvent::Chat {
                                        id: player_id,
                                        text,
                                    });
                                }
                            }
                            game_core::protocol::ClientMsg::SetName { name } => {
                                let name = name.trim().to_string();
                                if !name.is_empty() && name.len() <= 32 {
                                    let _ = event_tx_clone.send(RoomEvent::SetName {
                                        id: player_id,
                                        name,
                                    });
                                }
                            }
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    // Notify room of departure
    let _ = event_tx.send(RoomEvent::Leave { id: player_id });

    // Update room manager
    rooms.write().await.player_left(&room_code);

    log::info!("Player {} disconnected from room {}", player_id, room_code);
}
