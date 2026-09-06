pub mod adapter;
pub mod agent_runtime_state;
pub mod aop;
pub mod background_task;
pub mod credential;
pub mod cron;
pub mod crypto;
pub mod daily_jsonl;
pub mod external;
pub mod http;
pub mod jwt;
pub mod lark_integration;
pub mod logging;
pub mod monitoring;
pub mod password;
pub mod paths;
pub mod policy;
pub mod process;
pub mod wechat_ilink;
// 脱敏引擎实现在 common::redaction（前后端共享，单一事实源）；此处仅作路径兼容，
// 使 `crate::pkg::redaction::*` 与 `redaction::warmup()` 等既有引用继续可用。
pub use common::redaction;
pub mod request_context;
pub mod stats;
pub mod storage;
pub mod tool_log_retention;
pub mod tool_registry;
pub mod tool_tracing;
pub mod utils;
pub mod ws;

use common::config::AppConfig;

pub use request_context::*;

/// Initialize all pkg modules in one call.
/// Called from main.rs after config is loaded.
pub async fn init_all(config: &AppConfig) {
    // Initialize logging
    logging::init(config);

    // Initialize database storage
    storage::init(
        config.base_data_path().as_path(),
        &config.database,
        &config.stats,
    )
    .await;

    // Initialize JWT
    // 优先级：环境变量 JWT_SECRET → 配置文件 jwt.secret（首启可由环境变量固化）→ 内置默认
    let jwt_secret = std::env::var("JWT_SECRET")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            config
                .jwt
                .secret
                .clone()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "ai-orz-default-jwt-secret-change-me-in-production".to_string());
    // 优先级：环境变量 JWT_EXPIRY_HOURS → 配置文件 jwt.default_expiry_hours → 168
    let jwt_expiry_hours: i64 = std::env::var("JWT_EXPIRY_HOURS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .or_else(|| config.jwt.default_expiry_hours.map(|h| h as i64))
        .unwrap_or(168);
    jwt::init_jwt(&jwt_secret, jwt_expiry_hours);

    // Register all generic builtin tools to the global registry
    tool_registry::builtin::register_all(tool_registry::get_registry());
    sys_info!(
        "Registered {} generic builtin tools",
        tool_registry::builtin::GENERIC_BUILTIN_TOOLS.len()
    );

    // 预热脱敏预检自动机：让构建失败在启动期暴露，而不是首次响应时
    redaction::warmup();

    // Initialize tool call tracing logger (singleton factory)
    tool_tracing::logger::ToolCallLogger::init(config.base_data_path());

    sys_info!("All pkg modules initialized");
}

#[cfg(test)]
mod daily_jsonl_test;
#[cfg(test)]
mod logging_test;
#[cfg(test)]
mod request_context_test;
#[cfg(test)]
pub mod request_context_test_support;
