use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    env_logger::init();
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    log::info!("Server listening on {}", addr);

    let app = axum::Router::new();

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
