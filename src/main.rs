mod config;
mod error;
mod handlers;
mod middleware;
mod models;
mod pkg;
mod router;
mod service;

use crate::config::{AppConfig, load_config};

fn get_env_or_default(env_key: &str, default: &str) -> String {
    std::env::var(env_key).unwrap_or(default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载配置（默认配置嵌入在二进制中，不存在就自动生成）
    let config: AppConfig = load_config()?;

    // 初始化日志（使用配置中的路径）
    pkg::logging::init(&config);
    tracing::info!("Logging initialized, base data path: {}", config.base_data_path);

    // 获取数据库路径并初始化存储
    let db_path = config.db_path();
    pkg::storage::init(&db_path.to_str().unwrap())?;
    tracing::info!("Storage initialized: {:?}", db_path);

    // 初始化 service 层
    service::init();
    tracing::info!("Service layer initialized");

    // 环境变量覆盖 JWT 配置（JWT 机密敏感信息建议通过环境变量配置更安全）
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        "ai-orz-default-jwt-secret-change-me-in-production".to_string()
    });
    let jwt_expiry_hours: i64 = get_env_or_default("JWT_EXPIRY_HOURS", "168")
        .parse()
        .unwrap_or(168); // 默认 7 天过期（168 小时）
    pkg::jwt::init_jwt(&jwt_secret, jwt_expiry_hours);
    tracing::info!("JWT initialized, expiry: {} hours", jwt_expiry_hours);

    // 前端静态文件目录从配置读取，环境变量可覆盖
    let dist_dir = get_env_or_default("FRONTEND_DIST_DIR", &config.frontend.dist_dir);

    // 服务器监听地址从配置读取
    let server_addr = &config.server.listen_addr;

    // 启动服务器
    let app = router::create_router(&dist_dir);
    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tracing::info!("Server listening on {}, static files from {}", server_addr, dist_dir);

    axum::serve(listener, app).await?;

    Ok(())
}
