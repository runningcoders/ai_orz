use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tracing::info;

mod handlers;
mod service;
mod pkg;
mod error;
mod router;

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 构建路由
    let app = router::create_router();

    // 绑定地址
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Server listening on {}", addr);

    // 启动服务器
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
