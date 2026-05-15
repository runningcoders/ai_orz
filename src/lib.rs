//! ai_orz - AI 代理执行框架
//! 统一日志宏
//!
//! 核心机制：第一个参数是字符串字面量 → 无上下文模式；第一个参数非字符串 + 第二个是字符串 → 带上下文模式
//!
//! - 带 ctx:  log_info!(&ctx, "operation", "message {}", var)
//! - 无 ctx:  log_info!("message {}", var)

/// info 日志
#[macro_export]
macro_rules! log_info {
    // 无上下文: 第一个参数是字符串字面量（消息）
    ($msg:literal $(, $($fields:tt)*)?) => {{
        tracing::info!($msg $(, $($fields)*)?);
    }};
    
    // 带上下文: 第一个参数非字符串（ctx 或 &ctx），第二个参数是字符串字面量（operation）
    ($ctx:expr, $op:literal, $($fields:tt)*) => {{
        let span = tracing::info_span!(
            "request",
            log_id = %$ctx.log_id,
            user_id = %$ctx.user_id.as_deref().unwrap_or(""),
            username = %$ctx.username.as_deref().unwrap_or(""),
            organization_id = %$ctx.organization_id.as_deref().unwrap_or(""),
            agent_id = %$ctx.agent_id.as_deref().unwrap_or(""),
            task_id = %$ctx.task_id.as_deref().unwrap_or(""),
            project_id = %$ctx.project_id.as_deref().unwrap_or(""),
            operation = %$op
        );
        let _guard = span.enter();
        tracing::info!($($fields)*);
    }};
}

/// warn 日志
#[macro_export]
macro_rules! log_warn {
    // 无上下文: 第一个参数是字符串字面量（消息）
    ($msg:literal $(, $($fields:tt)*)?) => {{
        tracing::warn!($msg $(, $($fields)*)?);
    }};
    
    // 带上下文: 第一个参数非字符串（ctx 或 &ctx），第二个参数是字符串字面量（operation）
    ($ctx:expr, $op:literal, $($fields:tt)*) => {{
        let span = tracing::warn_span!(
            "request",
            log_id = %$ctx.log_id,
            user_id = %$ctx.user_id.as_deref().unwrap_or(""),
            username = %$ctx.username.as_deref().unwrap_or(""),
            organization_id = %$ctx.organization_id.as_deref().unwrap_or(""),
            agent_id = %$ctx.agent_id.as_deref().unwrap_or(""),
            task_id = %$ctx.task_id.as_deref().unwrap_or(""),
            project_id = %$ctx.project_id.as_deref().unwrap_or(""),
            operation = %$op
        );
        let _guard = span.enter();
        tracing::warn!($($fields)*);
    }};
}

/// error 日志
#[macro_export]
macro_rules! log_error {
    // 无上下文: 第一个参数是字符串字面量（消息）
    ($msg:literal $(, $($fields:tt)*)?) => {{
        tracing::error!($msg $(, $($fields)*)?);
    }};
    
    // 带上下文: 第一个参数非字符串（ctx 或 &ctx），第二个参数是字符串字面量（operation）
    ($ctx:expr, $op:literal, $($fields:tt)*) => {{
        let span = tracing::error_span!(
            "request",
            log_id = %$ctx.log_id,
            user_id = %$ctx.user_id.as_deref().unwrap_or(""),
            username = %$ctx.username.as_deref().unwrap_or(""),
            organization_id = %$ctx.organization_id.as_deref().unwrap_or(""),
            agent_id = %$ctx.agent_id.as_deref().unwrap_or(""),
            task_id = %$ctx.task_id.as_deref().unwrap_or(""),
            project_id = %$ctx.project_id.as_deref().unwrap_or(""),
            operation = %$op
        );
        let _guard = span.enter();
        tracing::error!($($fields)*);
    }};
}

/// debug 日志
#[macro_export]
macro_rules! log_debug {
    // 无上下文: 第一个参数是字符串字面量（消息）
    ($msg:literal $(, $($fields:tt)*)?) => {{
        tracing::debug!($msg $(, $($fields)*)?);
    }};
    
    // 带上下文: 第一个参数非字符串（ctx 或 &ctx），第二个参数是字符串字面量（operation）
    ($ctx:expr, $op:literal, $($fields:tt)*) => {{
        let span = tracing::debug_span!(
            "request",
            log_id = %$ctx.log_id,
            user_id = %$ctx.user_id.as_deref().unwrap_or(""),
            username = %$ctx.username.as_deref().unwrap_or(""),
            organization_id = %$ctx.organization_id.as_deref().unwrap_or(""),
            agent_id = %$ctx.agent_id.as_deref().unwrap_or(""),
            task_id = %$ctx.task_id.as_deref().unwrap_or(""),
            project_id = %$ctx.project_id.as_deref().unwrap_or(""),
            operation = %$op
        );
        let _guard = span.enter();
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
