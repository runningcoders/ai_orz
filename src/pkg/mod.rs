pub mod daily_jsonl;
pub mod external;
pub mod jwt;
pub mod logging;
pub mod monitoring;
pub mod request_context;
pub mod storage;
pub mod tool_registry;
pub mod tool_tracing;

use common::config::AppConfig;
use common::bail_err;

pub use request_context::*;

/// Initialize all pkg modules in one call.
/// Called from main.rs after config is loaded.
pub async fn init_all(config: &AppConfig) {
    // Initialize logging
    logging::init(config);

    // Initialize database storage
    storage::init(config.base_data_path().as_path(), &config.database).await;

    // Initialize JWT
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "ai-orz-default-jwt-secret-change-me-in-production".to_string());
    let jwt_expiry_hours: i64 = get_env_or_default("JWT_EXPIRY_HOURS", "168")
        .parse()
        .unwrap_or(168);
    jwt::init_jwt(&jwt_secret, jwt_expiry_hours);

    // Initialize tool call tracing logger (singleton factory)
    tool_tracing::logger::ToolCallLogger::init(config.base_data_path());

    sys_info!("All pkg modules initialized");
}

fn get_env_or_default(env_key: &str, default: &str) -> String {
    std::env::var(env_key).unwrap_or(default.to_string())
}

#[cfg(test)]
mod daily_jsonl_test;
#[cfg(test)]
mod logging_test;
#[cfg(test)]
mod request_context_test;
