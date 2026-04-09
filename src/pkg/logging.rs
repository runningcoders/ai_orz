// ==================== 日志配置 ====================
//! 日志模块：同时输出到控制台和按日期自动分割的日志文件
//!
//! - 控制台输出：方便开发调试
//! - 文件输出：按日期自动滚动，持久化日志
//! - 日志路径从应用配置读取，支持自定义数据目录

use crate::config::AppConfig;
use common::constants::RequestContext;
use tracing_subscriber::{
    fmt::{self},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use tracing_appender::rolling;

/// 初始化日志系统
///
/// - 同时输出到控制台和配置的日志目录下按日期自动分割的日志文件
/// - 自动按日期滚动，不会产生过大日志文件
pub fn init(config: &AppConfig) {
    let fmt_layer_console = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true);

    // 如果启用文件日志，输出到配置的目录
    if config.logging.enable_file_log {
        let logs_dir = config.log_dir();
        // 创建日志目录（如果不存在，load_config 已经创建过，但保险起见再检查一次）
        if !logs_dir.exists() {
            std::fs::create_dir_all(&logs_dir)
                .expect(&format!("Failed to create logs directory at {:?}", logs_dir));
        }

        // 文件输出层：按日期自动滚动，每天新建一个日志文件
        let file_appender = rolling::daily(&logs_dir, "ai_orz.log");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        let fmt_layer_file = fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true)
            .with_writer(non_blocking);

        let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer_console)
            .with(fmt_layer_file)
            .init();

        // _guard 需要保持活着，这样 NonBlocking 才能正常工作
        // 因为我们是服务启动时初始化一次，所以这里用 static mut 持有
        // 需要注意：这个守卫只要不 drop 就会一直工作，服务启动后不会 drop，所以没问题
        static mut _GUARD: Option<tracing_appender::non_blocking::WorkerGuard> = None;
        unsafe {
            _GUARD = Some(_guard);
        }
    } else {
        // 只输出到控制台
        let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer_console)
            .init();
    }
}

/// 创建带 context 信息的 span
fn create_span(operation: &str, ctx: &RequestContext) -> tracing::Span {
    tracing::info_span!(
        "request",
        log_id = %ctx.log_id,
        user_id = %ctx.uid(),
        username = %ctx.uname(),
        operation = %operation
    )
}

/// 带上下文记录 info 日志（值传递，只读）
pub fn info(ctx: RequestContext, operation: &str, msg: &str) {
    let span = create_span(operation, &ctx);
    let _guard = span.enter();
    tracing::info!(msg);
}

/// 带上下文记录 error 日志（值传递，只读）
pub fn log_error(ctx: RequestContext, operation: &str, msg: &str) {
    let span = create_span(operation, &ctx);
    let _guard = span.enter();
    tracing::error!(msg);
}

/// 带上下文记录 warn 日志（值传递，只读）
pub fn warn(ctx: RequestContext, operation: &str, msg: &str) {
    let span = create_span(operation, &ctx);
    let _guard = span.enter();
    tracing::warn!(msg);
}

/// 带上下文记录 debug 日志（值传递，只读）
pub fn debug(ctx: RequestContext, operation: &str, msg: &str) {
    let span = create_span(operation, &ctx);
    let _guard = span.enter();
    tracing::debug!(msg);
}
