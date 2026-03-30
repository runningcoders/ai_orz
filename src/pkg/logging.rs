// ==================== 日志配置 ====================

use crate::pkg::RequestContext;
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

/// 带上下文记录 info 日志
pub fn info(ctx: &RequestContext, operation: &str, msg: &str) {
    let span = create_span(operation, ctx);
    let _guard = span.enter();
    tracing::info!(msg)
}

/// 带上下文记录 error 日志
pub fn error(ctx: &RequestContext, operation: &str, msg: &str) {
    let span = create_span(operation, ctx);
    let _guard = span.enter();
    tracing::error!(msg)
}

/// 带上下文记录 warn 日志
pub fn warn(ctx: &RequestContext, operation: &str, msg: &str) {
    let span = create_span(operation, ctx);
    let _guard = span.enter();
    tracing::warn!(msg)
}

/// 带上下文记录 debug 日志
pub fn debug(ctx: &RequestContext, operation: &str, msg: &str) {
    let span = create_span(operation, ctx);
    let _guard = span.enter();
    tracing::debug!(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::RequestContext;

    #[test]
    fn test_log_with_context() {
        init();
        let ctx = RequestContext::generate_new(Some("user123".to_string()), Some("test_user".to_string()));
        info(&ctx, "test", "这是一条测试日志");
    }
}
