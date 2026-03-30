mod error;
mod handlers;
mod models;
mod pkg;
mod router;
mod service;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 初始化数据库
    let _db = pkg::storage::init_from_config("ai_orz.toml")?;
    tracing::info!("Database initialized: data/ai_orz.db");

    // 初始化 service 层（DAO 单例等）
    service::init();
    tracing::info!("Service layer initialized");

    // 启动服务器
    let app = router::create_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server listening on 0.0.0.0:3000");

    axum::serve(listener, app).await?;

    Ok(())
}
