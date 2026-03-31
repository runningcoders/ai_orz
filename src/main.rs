mod error;
mod handlers;
mod models;
mod pkg;
mod router;
mod service;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    pkg::logging::init();

    // 初始化存储（从配置文件读取数据库路径）
    let db_path = "data/ai_orz.db";
    pkg::storage::init(db_path)?;
    tracing::info!("Storage initialized: {}", db_path);

    // 初始化 service 层
    service::init();
    tracing::info!("Service layer initialized");

    // 启动服务器
    let app = router::create_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server listening on 0.0.0.0:3000");

    axum::serve(listener, app).await?;

    Ok(())
}
