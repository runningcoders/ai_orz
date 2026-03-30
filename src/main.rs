use axum::Router;
use std::net::SocketAddr;
use tracing::info;

mod handlers;
mod models;
mod service;
mod pkg;
mod error;
mod router;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    let app = router::create_router();
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
