//! ai_orz - AI 代理执行框架
//! 统一日志宏
//!
//! 核心机制：第一个参数是字符串字面量 → 无上下文模式；第一个参数非字符串 + 第二个是字符串 → 带上下文模式
//!
//! - 带 ctx:  log_info!(&ctx, "operation", "message {}", var)
//! - 无 ctx:  log_info!("message {}", var)
//!
//! 字段注入通过 `#[derive(LogFields)]` 自动生成，新增上下文字段只需在 RequestContext 上加 `#[log_field]`。

/// info 日志
#[macro_export]
macro_rules! log_info {
    // 无上下文: 第一个参数是字符串字面量（消息）
    ($msg:literal $(, $($fields:tt)*)?) => {{
        tracing::info!($msg $(, $($fields)*)?);
    }};

    // 带上下文: 第一个参数非字符串（ctx 或 &ctx），第二个参数是字符串字面量（operation）
    ($ctx:expr, $op:literal, $($fields:tt)*) => {{
        use $crate::pkg::logging::LogFields;
        let span = ($ctx).create_log_span($op, tracing::Level::INFO);
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
        use $crate::pkg::logging::LogFields;
        let span = ($ctx).create_log_span($op, tracing::Level::WARN);
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
        use $crate::pkg::logging::LogFields;
        let span = ($ctx).create_log_span($op, tracing::Level::ERROR);
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
        use $crate::pkg::logging::LogFields;
        let span = ($ctx).create_log_span($op, tracing::Level::DEBUG);
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
use crate::pkg::aop;

pub mod config;
pub mod consumer;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod producer;
pub mod router;
pub mod service;

/// 应用程序入口函数
///
/// bin target (main.rs) 通过 `ai_orz::run()` 调用此函数，
/// 避免在 bin target 中重新声明 mod 导致代码被编译两次。
pub async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    config::init()?;
    let config = config::get();

    // 初始化所有 pkg 模块
    pkg::init_all(&config).await;
    sys_info!(
        "Logging & storage & JWT & tool registry initialized, base data path: {}",
        config.base_data_path().display()
    );

    // 初始化 service 层
    service::init();
    sys_info!("Service layer initialized");

    // 初始化业务生产者（注册到 AOP）
    producer::init().await?;
    sys_info!("Business producers registered");

    // 初始化业务消费者（注册到 AOP）
    consumer::init().await?;
    sys_info!("Business consumers registered");

    // 启动 AOP 调度器（轮询生产者 + 异步消费者 worker）
    aop::init_all().await?;
    sys_info!("AOP scheduler started");

    // 前端静态文件目录从配置读取，环境变量可覆盖
    let dist_dir =
        std::env::var("FRONTEND_DIST_DIR").unwrap_or_else(|_| config.frontend.dist_dir.clone());

    // 服务器监听地址从配置读取
    let server_addr = &config.server.listen_addr;

    // 启动服务器
    let app = router::create_router(&dist_dir, config.clone());
    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    sys_info!(
        "Server listening on {}, static files from {}",
        server_addr,
        dist_dir
    );

    axum::serve(listener, app).await?;

    Ok(())
}
