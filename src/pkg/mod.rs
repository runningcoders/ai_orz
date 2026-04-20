pub mod daily_jsonl;
pub mod external;
pub mod logging;
pub mod jwt;
pub mod storage;
pub mod request_context;
pub mod tool_registry;
pub mod tool_call_logging;

use common::config::AppConfig;

pub use request_context::*;

/// Initialize all pkg modules in one call.
/// Called from main.rs after config is loaded.
pub async fn init_all(config: &AppConfig) {
    // Initialize logging
    logging::init(config);

    // Initialize database storage
    let db_path = config.db_path();
    storage::init(&db_path.to_str().unwrap()).await;

    // Initialize JWT
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "ai-orz-default-jwt-secret-change-me-in-production".to_string());
    let jwt_expiry_hours: i64 = get_env_or_default("JWT_EXPIRY_HOURS", "168")
        .parse()
        .unwrap_or(168);
    jwt::init_jwt(&jwt_secret, jwt_expiry_hours);

    // Initialize global tool registry
    tool_registry::init();

    tracing::info!("All pkg modules initialized");
}

fn get_env_or_default(env_key: &str, default: &str) -> String {
    std::env::var(env_key).unwrap_or(default.to_string())
}

#[cfg(test)]
mod logging_test;
#[cfg(test)]
mod request_context_test;
#[cfg(test)]
mod daily_jsonl_test;
#[cfg(test)]
mod tool_call_logging_test;
