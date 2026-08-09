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

    // 先初始化业务生产者/消费者（注册到 AOP）——把事件总线基础设施就位，
    // 避免后续基础数据初始化阶段误 publish 的事件找不到订阅者。
    producer::init().await?;
    sys_info!("Business producers registered");
    consumer::init().await?;
    sys_info!("Business consumers registered");

    // 第二阶段：service 层各 Domain 的基础数据（幂等，需要 DB IO，失败仅记 warn）
    service::init_base_data().await;
    sys_info!("Service base data initialized (idempotent defaults)");

    // 创建 AOP 统计收集器并注入 Hook（在 worker 启动前）
    let aop_stats_collector = consumer::AopStatsCollector::new();
    {
        use std::sync::Arc;
        let hook = Arc::new(consumer::AopStatsHook::new(aop_stats_collector.clone()))
            as Arc<dyn crate::pkg::aop::AopMetricsHook>;
        crate::pkg::aop::registry().set_metrics_hook(hook);
        sys_info!("AOP stats hook installed");
    }
    // 把 collector 注入 SystemDomain（供后续查询）
    crate::service::domain::system::set_aop_stats_collector(aop_stats_collector);

    // 启动 AOP 调度器（轮询生产者 + 异步消费者 worker）
    aop::init_all().await?;
    sys_info!("AOP scheduler started");

    // 前端静态文件目录从配置读取，环境变量可覆盖
    let dist_dir =
        std::env::var("FRONTEND_DIST_DIR").unwrap_or_else(|_| config.frontend.dist_dir.clone());

    // 服务器监听地址从配置读取
    let server_addr = &config.server.listen_addr;

    // 启动服务器（带优雅退出：信号触发后停止接受新连接并排空在途请求）
    let app = router::create_router(&dist_dir, config.clone());
    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    sys_info!(
        "Server listening on {}, static files from {}",
        server_addr,
        dist_dir
    );

    // 优雅退出：信号触发 → axum 停止接受新连接并排空在途请求；
    // SSE/WS 长连接不会主动关闭，排空设 10s 上限（从信号触发时刻起算），
    // 超时后丢弃 serve future 强制退出（进程即将退出，长连接自然断开）。
    // 注意：超时必须从信号触发后才开始计时，不能包在 serve 外层——否则启动 N 秒后被误杀
    let (sig_tx, mut sig_rx) = tokio::sync::oneshot::channel::<()>();
    let serve_future = axum::serve(listener, app).with_graceful_shutdown(async move {
        wait_shutdown_trigger().await;
        sys_info!("Shutdown signal received, stopping server...");
        let _ = sig_tx.send(());
    });
    const SHUTDOWN_GRACE_SECS: u64 = 10;
    let drain_deadline = async {
        // 信号未触发时 sig_rx 永不就绪，计时不会开始
        let _ = sig_rx.await;
        tokio::time::sleep(std::time::Duration::from_secs(SHUTDOWN_GRACE_SECS)).await;
    };
    tokio::select! {
        serve_result = serve_future => serve_result?,
        _ = drain_deadline => sys_warn!(
            "Graceful shutdown drain window ({}s) elapsed, forcing shutdown",
            SHUTDOWN_GRACE_SECS
        ),
    }
    sys_info!("HTTP server stopped");

    // 业务关停编排：渠道停服 → AOP worker/producer 退出 → stats 落盘 → DB 连接池关闭
    shutdown_services().await;
    sys_info!("Shutdown complete, goodbye");

    Ok(())
}

/// 阻塞直到收到任一退出信号：Ctrl+C（SIGINT）/ SIGTERM / SIGQUIT
async fn wait_shutdown_trigger() {
    let ctrl_c = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {}
            Err(e) => sys_error!("Failed to install Ctrl+C handler: {}", e),
        }
    };

    #[cfg(unix)]
    {
        use tokio::signal::unix::SignalKind;
        tokio::select! {
            _ = ctrl_c => {}
            // SIGINT 已由 ctrl_c 覆盖，这里只补 SIGTERM / SIGQUIT
            _ = wait_unix_signal(SignalKind::terminate()) => {}
            _ = wait_unix_signal(SignalKind::quit()) => {}
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}

/// 等待单个 unix 信号触发；安装失败时永久挂起（不误触发停机）
#[cfg(unix)]
async fn wait_unix_signal(kind: tokio::signal::unix::SignalKind) {
    match tokio::signal::unix::signal(kind) {
        Ok(mut sig) => {
            let _ = sig.recv().await;
        }
        Err(e) => {
            sys_error!("Failed to install signal handler: {}", e);
            std::future::pending::<()>().await;
        }
    }
}

/// 业务关停编排（HTTP server 停止接受新连接后调用）
///
/// 顺序：
/// 1. 消息渠道入站监听停服（飞书 WS 等外部长连接）
/// 2. AOP worker/producer 退出（当前事件处理完即退）
/// 3. 等待后台协程排空
/// 4. DuckDB 统计缓冲 flush 落盘
/// 5. SQLite 连接池关闭
async fn shutdown_services() {
    use crate::pkg::storage;

    if let Err(e) = crate::producer::message_channel::shutdown().await {
        sys_error!("Message channel shutdown error: {}", e);
    } else {
        sys_info!("Message channels stopped");
    }

    if let Err(e) = aop::shutdown_all().await {
        sys_error!("AOP shutdown error: {}", e);
    } else {
        sys_info!("AOP workers & producers stopped");
    }

    // 给 worker/producer 排空窗口（worker 最多一个 empty_queue_sleep + 当前事件处理时长）
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let storage = storage::get();
    if let Some(stats) = storage.stats_opt() {
        match stats.flush_all(crate::pkg::RequestContext::new_system()).await {
            Ok(()) => sys_info!("Stats flushed to DuckDB"),
            Err(e) => sys_error!("Stats flush error: {}", e),
        }
    }

    storage.sqlite_pool().close().await;
    sys_info!("SQLite pool closed");
}
