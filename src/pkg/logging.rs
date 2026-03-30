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
pub fn create_span(operation: &str, ctx: &RequestContext) -> tracing::Span {
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

// ==================== 日志模块测试 ====================

#[cfg(test)]
mod tests {
    use crate::pkg::RequestContext;
    use crate::pkg::{info, warn, error, debug};

    fn new_ctx() -> RequestContext {
        RequestContext::generate_new(None, None)
    }

    fn new_ctx_with_user() -> RequestContext {
        RequestContext::generate_new(
            Some("test_user".to_string()),
            Some("test_name".to_string()),
        )
    }

    #[test]
    fn test_info_log() {
        info(&new_ctx(), "test_info", "这是一条 info 日志");
    }

    #[test]
    fn test_warn_log() {
        warn(&new_ctx(), "test_warn", "这是一条 warn 日志");
    }

    #[test]
    fn test_error_log() {
        error(&new_ctx(), "test_error", "这是一条 error 日志");
    }

    #[test]
    fn test_debug_log() {
        debug(&new_ctx(), "test_debug", "这是一条 debug 日志");
    }

    #[test]
    fn test_log_with_empty_user() {
        let ctx = new_ctx();
        assert_eq!(ctx.uid(), "");
        assert_eq!(ctx.uname(), "");
        info(&ctx, "anonymous", "匿名用户访问");
    }

    #[test]
    fn test_log_id_format() {
        let ctx = new_ctx();
        let log_id = &ctx.log_id;
        assert_eq!(log_id.len(), 20, "log_id 长度应为20位");
        assert!(log_id.chars().all(|c| c.is_ascii_digit()), "log_id 应为纯数字");
    }

    #[test]
    fn test_log_id_uniqueness() {
        let ctx1 = new_ctx();
        let ctx2 = new_ctx();
        println!("ctx1: {}, ctx2: {}", ctx1.log_id, ctx2.log_id);
    }

    #[test]
    fn test_log_id_from_header() {
        use axum::http::HeaderValue;
        
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::HeaderName::from_static("x-log-id"),
            HeaderValue::from_static("20260331013000000123"),
        );
        headers.insert(
            axum::http::header::HeaderName::from_static("x-user-id"),
            HeaderValue::from_static("user_001"),
        );
        headers.insert(
            axum::http::header::HeaderName::from_static("x-username"),
            HeaderValue::from_static("zhang_san"),
        );
        
        let ctx = RequestContext::from_headers(&headers);
        
        assert_eq!(ctx.log_id, "20260331013000000123");
        assert_eq!(ctx.uid(), "user_001");
        assert_eq!(ctx.uname(), "zhang_san");
    }

    #[test]
    fn test_log_id_auto_generate_when_missing() {
        let headers = axum::http::HeaderMap::new();
        let ctx = RequestContext::from_headers(&headers);
        
        assert!(!ctx.log_id.is_empty());
        assert_eq!(ctx.log_id.len(), 20);
    }

    #[test]
    fn test_multiple_logs_same_context() {
        let ctx = new_ctx_with_user();
        
        info(&ctx, "step1", "第一步操作");
        info(&ctx, "step2", "第二步操作");
        info(&ctx, "step3", "第三步操作完成");
        
        assert_eq!(ctx.log_id.len(), 20);
    }

    #[test]
    fn test_context_uid_helper() {
        let ctx_with_user = new_ctx_with_user();
        assert_eq!(ctx_with_user.uid(), "test_user");
        
        let ctx_without_user = new_ctx();
        assert_eq!(ctx_without_user.uid(), "");
    }

    #[test]
    fn test_context_uname_helper() {
        let ctx_with_name = new_ctx_with_user();
        assert_eq!(ctx_with_name.uname(), "test_name");
        
        let ctx_without_name = new_ctx();
        assert_eq!(ctx_without_name.uname(), "");
    }

    #[test]
    fn test_context_clone() {
        let ctx1 = new_ctx_with_user();
        let ctx2 = ctx1.clone();
        
        assert_eq!(ctx1.log_id, ctx2.log_id);
        assert_eq!(ctx1.uid(), ctx2.uid());
        assert_eq!(ctx1.uname(), ctx2.uname());
    }

    #[test]
    fn test_long_operation_name() {
        let ctx = new_ctx();
        info(&ctx, "create_agent_with_validation", "创建 Agent 并验证");
    }

    #[test]
    fn test_chinese_message() {
        let ctx = new_ctx();
        info(&ctx, "test", "这是一条中文测试日志消息");
        error(&ctx, "test", "错误信息：数据库连接失败");
        warn(&ctx, "test", "警告：内存使用率超过 80%");
    }

    #[test]
    fn test_special_characters_in_message() {
        let ctx = new_ctx();
        info(&ctx, "test", r#"特殊字符: @#$%^&*()_+-=[]{}|;':",./<>?"#);
        info(&ctx, "test", r#"JSON: {"key": "value"}"#);
    }

    #[test]
    fn test_empty_message() {
        let ctx = new_ctx();
        info(&ctx, "test", "");
    }

    #[test]
    fn test_very_long_message() {
        let ctx = new_ctx();
        let long_msg = "A".repeat(1000);
        info(&ctx, "test", &long_msg);
    }
}
