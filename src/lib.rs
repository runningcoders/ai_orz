//! ai_orz - AI 代理执行框架
//! 统一日志宏
//!
//! 使用方式（自动检测是否带上下文）：
//! - 带 ctx:  log_info!(&ctx, "operation", "message {}", var)
//! - 无 ctx:  log_info!("message {}", var)

/// info 日志（自动检测是否带上下文）
#[macro_export]
macro_rules! log_info {
    // 带上下文: log_info!(&ctx, "operation", ...)
    ($ctx:expr, $op:literal $(, $($fields:tt)*)?) => {{
        use $crate::pkg::logging::create_span;
        let span = create_span($op, $ctx);
        let _guard = span.enter();
        tracing::info!($($($fields)*)?);
    }};
    
    // 无上下文: log_info!("message {}", var)
    ($($fields:tt)*) => {{
        tracing::info!($($fields)*);
    }};
}

/// warn 日志（自动检测是否带上下文）
#[macro_export]
macro_rules! log_warn {
    // 带上下文: log_warn!(&ctx, "operation", ...)
    ($ctx:expr, $op:literal $(, $($fields:tt)*)?) => {{
        use $crate::pkg::logging::create_span;
        let span = create_span($op, $ctx);
        let _guard = span.enter();
        tracing::warn!($($($fields)*)?);
    }};
    
    // 无上下文: log_warn!("message {}", var)
    ($($fields:tt)*) => {{
        tracing::warn!($($fields)*);
    }};
}

/// error 日志（自动检测是否带上下文）
#[macro_export]
macro_rules! log_error {
    // 带上下文: log_error!(&ctx, "operation", ...)
    ($ctx:expr, $op:literal $(, $($fields:tt)*)?) => {{
        use $crate::pkg::logging::create_span;
        let span = create_span($op, $ctx);
        let _guard = span.enter();
        tracing::error!($($($fields)*)?);
    }};
    
    // 无上下文: log_error!("message {}", var)
    ($($fields:tt)*) => {{
        tracing::error!($($fields)*);
    }};
}

/// debug 日志（自动检测是否带上下文）
#[macro_export]
macro_rules! log_debug {
    // 带上下文: log_debug!(&ctx, "operation", ...)
    ($ctx:expr, $op:literal $(, $($fields:tt)*)?) => {{
        use $crate::pkg::logging::create_span;
        let span = create_span($op, $ctx);
        let _guard = span.enter();
        tracing::debug!($($($fields)*)?);
    }};
    
    // 无上下文: log_debug!("message {}", var)
    ($($fields:tt)*) => {{
        tracing::debug!($($fields)*);
    }};
}

// 兼容旧代码的别名（可以逐步删除）
#[macro_export]
macro_rules! sys_info {
    ($($tt:tt)*) => { $crate::log_info!($($tt)*) };
}
#[macro_export]
macro_rules! sys_warn {
    ($($tt:tt)*) => { $crate::log_warn!($($tt)*) };
}
#[macro_export]
macro_rules! sys_error {
    ($($tt:tt)*) => { $crate::log_error!($($tt)*) };
}
#[macro_export]
macro_rules! sys_debug {
    ($($tt:tt)*) => { $crate::log_debug!($($tt)*) };
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
