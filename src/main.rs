mod config;
mod error;
mod handlers;
mod middleware;
mod models;
mod pkg;
mod router;
mod service;

fn get_env_or_default(env_key: &str, default: &str) -> String {
    std::env::var(env_key).unwrap_or(default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    config::init()?;
    let config = config::get();
    
    // Initialize all pkg modules in one call
    pkg::init_all(&config).await;
    tracing::info!(
        "Logging & storage & JWT & tool registry initialized, base data path: {}",
        config.base_data_path
    );

    // 初始化 service 层
    service::init();
    tracing::info!("Service layer initialized");

    // 前端静态文件目录从配置读取，环境变量可覆盖
    let dist_dir = get_env_or_default("FRONTEND_DIST_DIR", &config.frontend.dist_dir);

    // 服务器监听地址从配置读取
    let server_addr = &config.server.listen_addr;

    // 启动服务器
    let app = router::create_router(&dist_dir);
    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tracing::info!(
        "Server listening on {}, static files from {}",
        server_addr,
        dist_dir
    );

    axum::serve(listener, app).await?;

    Ok(())
}
