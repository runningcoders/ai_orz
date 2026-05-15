//! ai_orz - AI 代理执行框架

// 统一日志宏：两套宏，明确区分
// - 业务代码（带 ctx）：log_info!(ctx, "op", "msg {}", var)
// - 初始化代码（无 ctx）：sys_info!("msg {}", var)

/// 带上下文 info 日志宏
#[macro_export]
macro_rules! log_info {
    ($ctx:expr, $operation:expr, $($fields:tt)*) => {{
        use $crate::pkg::logging::create_span;
        let span = create_span($operation, $ctx);
        let _guard = span.enter();
        tracing::info!($($fields)*);
    }};
}

/// 带上下文 warn 日志宏
#[macro_export]
macro_rules! log_warn {
    ($ctx:expr, $operation:expr, $($fields:tt)*) => {{
        use $crate::pkg::logging::create_span;
        let span = create_span($operation, $ctx);
        let _guard = span.enter();
        tracing::warn!($($fields)*);
    }};
}

/// 带上下文 error 日志宏
#[macro_export]
macro_rules! log_error {
    ($ctx:expr, $operation:expr, $($fields:tt)*) => {{
        use $crate::pkg::logging::create_span;
        let span = create_span($operation, $ctx);
        let _guard = span.enter();
        tracing::error!($($fields)*);
    }};
}

/// 带上下文 debug 日志宏
#[macro_export]
macro_rules! log_debug {
    ($ctx:expr, $operation:expr, $($fields:tt)*) => {{
        use $crate::pkg::logging::create_span;
        let span = create_span($operation, $ctx);
        let _guard = span.enter();
        tracing::debug!($($fields)*);
    }};
}

/// 系统 info 日志（无 ctx，仅用于初始化）
#[macro_export]
macro_rules! sys_info {
    ($($args:tt)*) => {{ tracing::info!($($args)*); }};
}

/// 系统 warn 日志（无 ctx，仅用于初始化）
#[macro_export]
macro_rules! sys_warn {
    ($($args:tt)*) => {{ tracing::warn!($($args)*); }};
}

/// 系统 error 日志（无 ctx，仅用于初始化）
#[macro_export]
macro_rules! sys_error {
    ($($args:tt)*) => {{ tracing::error!($($args)*); }};
}

/// 系统 debug 日志（无 ctx，仅用于初始化）
#[macro_export]
macro_rules! sys_debug {
    ($($args:tt)*) => {{ tracing::debug!($($args)*); }};
}

// pkg 模块必须在宏之后声明，因为 pkg 内部使用 sys_info! 宏
pub mod pkg;

pub mod config;
pub mod consumer;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod router;
pub mod service;
