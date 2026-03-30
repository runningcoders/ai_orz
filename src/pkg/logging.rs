// ==================== 日志配置 ====================

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 初始化日志系统
pub fn init() {
    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true);

    let filter_layer = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();
}

/// 创建带 log_id 的 span
pub fn create_span(operation: &str, log_id: &str) -> tracing::Span {
    tracing::info_span!(
        "request",
        log_id = %log_id,
        operation = %operation
    )
}

/// 带 log_id 记录 info 日志
pub fn log_info(log_id: &str, operation: &str, msg: &str) {
    let span = create_span(operation, log_id);
    let _guard = span.enter();
    tracing::info!(msg)
}

/// 带 log_id 记录 error 日志
pub fn log_error(log_id: &str, operation: &str, msg: &str) {
    let span = create_span(operation, log_id);
    let _guard = span.enter();
    tracing::error!(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log() {
        init();
        log_info("abc123", "test", "这是一条测试日志");
    }
}
