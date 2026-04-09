mod error;
mod handlers;
mod middleware;
mod models;
mod pkg;
mod router;
mod service;

use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    server: ServerConfig,
    database: DatabaseConfig,
    frontend: FrontendConfig,
    jwt: Option<JwtConfig>,
}

#[derive(Deserialize)]
struct JwtConfig {
    secret: Option<String>,
    default_expiry_hours: Option<i64>,
}

#[derive(Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Deserialize)]
struct DatabaseConfig {
    url: String,
}

#[derive(Deserialize)]
struct FrontendConfig {
    dist_dir: String,
}

fn get_env_or(config_value: &str, env_key: &str) -> String {
    std::env::var(env_key).unwrap_or(config_value.to_string())
}

fn get_env_or_default(env_key: &str, default: &str) -> String {
    std::env::var(env_key).unwrap_or(default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    pkg::logging::init();

    // 读取配置文件
    let config: Config = match std::fs::read_to_string("ai_orz.toml") {
        Ok(content) => toml::from_str(&content)?,
        Err(_) => {
            tracing::warn!("ai_orz.toml not found, using default configuration");
            Config {
                server: ServerConfig {
                    host: "0.0.0.0".to_string(),
                    port: 3000,
                },
                database: DatabaseConfig {
                    url: "data/ai_orz.db".to_string(),
                },
                frontend: FrontendConfig {
                    dist_dir: "dist".to_string(),
                },
                jwt: None,
            }
        }
    };

    // 环境变量覆盖配置文件
    let server_host = get_env_or(&config.server.host, "SERVER_HOST");
    let server_port: u16 = get_env_or_default("SERVER_PORT", &config.server.port.to_string())
        .parse()
        .unwrap_or(config.server.port);
    let db_path = get_env_or(&config.database.url, "DATABASE_URL");
    let dist_dir = get_env_or(&config.frontend.dist_dir, "FRONTEND_DIST_DIR");

    let server_addr = format!("{}:{}", server_host, server_port);

    // 初始化存储
    pkg::storage::init(&db_path)?;
    tracing::info!("Storage initialized: {}", db_path);

    // 初始化 service 层
    service::init();
    tracing::info!("Service layer initialized");

    // 初始化 JWT
    let jwt_secret = config.jwt.as_ref()
        .and_then(|j| j.secret.as_ref())
        .map(|s| s.clone())
        .unwrap_or_else(|| {
            // 如果配置文件没有提供，尝试从环境变量获取，还是没有就用默认
            std::env::var("JWT_SECRET").unwrap_or_else(|_| "ai-orz-default-jwt-secret-change-me-in-production".to_string())
        });
    let jwt_expiry_hours = config.jwt.as_ref()
        .and_then(|j| j.default_expiry_hours)
        .unwrap_or(24 * 7); // 默认 7 天过期
    pkg::jwt::init_jwt(&jwt_secret, jwt_expiry_hours);
    tracing::info!("JWT initialized, expiry: {} hours", jwt_expiry_hours);

    // 启动服务器
    let app = router::create_router(&dist_dir);
    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tracing::info!("Server listening on {}, static files from {}", server_addr, dist_dir);

    axum::serve(listener, app).await?;

    Ok(())
}
