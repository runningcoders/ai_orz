mod error;
mod handlers;
mod models;
mod pkg;
mod router;
mod service;

fn get_env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or(default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    pkg::logging::init();

    // 读取配置
    let server_host = get_env("SERVER_HOST", "0.0.0.0");
    let server_port = get_env("SERVER_PORT", "3000");
    let db_path = get_env("DATABASE_URL", "data/ai_orz.db");

    let server_addr = format!("{}:{}", server_host, server_port);

    // 初始化存储
    pkg::storage::init(&db_path)?;
    tracing::info!("Storage initialized: {}", db_path);

    // 初始化 service 层
    service::init();
    tracing::info!("Service layer initialized");

    // 启动服务器
    let app = router::create_router();
    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tracing::info!("Server listening on {}", server_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
