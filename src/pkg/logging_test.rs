//! 日志模块单元测试

use common::constants::RequestContext;
use crate::pkg::logging::{debug, info, log_error, warn};

fn new_ctx() -> RequestContext {
    RequestContext::new(None, None)
}

fn new_ctx_with_user() -> RequestContext {
    RequestContext::new(Some("test_user".to_string()), Some("test_name".to_string()))
}

#[test]
fn test_info_log() {
    info(new_ctx(), "test_info", "这是一条 info 日志");
}

#[test]
fn test_warn_log() {
    warn(new_ctx(), "test_warn", "这是一条 warn 日志");
}

#[test]
fn test_error_log() {
    log_error(new_ctx(), "test_error", "这是一条 error 日志");
}

#[test]
fn test_debug_log() {
    debug(new_ctx(), "test_debug", "这是一条 debug 日志");
}

#[test]
fn test_log_with_empty_user() {
    let ctx = new_ctx();
    assert_eq!(ctx.uid(), "");
    assert_eq!(ctx.uname(), "");
    info(ctx, "anonymous", "匿名用户访问");
}

#[test]
fn test_log_id_format() {
    let ctx = new_ctx();
    let log_id = &ctx.log_id;
    assert_eq!(log_id.len(), 20, "log_id 长度应为20位");
    assert!(
        log_id.chars().all(|c: char| c.is_ascii_digit()),
        "log_id 应为纯数字"
    );
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

    info(ctx.clone(), "step1", "第一步操作");
    info(ctx.clone(), "step2", "第二步操作");
    info(ctx.clone(), "step3", "第三步操作完成");

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
    info(ctx, "create_agent_with_validation", "创建 Agent 并验证");
}

#[test]
fn test_chinese_message() {
    let ctx = new_ctx();
    info(ctx.clone(), "test", "这是一条中文测试日志消息");
    log_error(ctx.clone(), "test", "错误信息：数据库连接失败");
    warn(ctx.clone(), "test", "警告：内存使用率超过 80%");
}

#[test]
fn test_special_characters_in_message() {
    let ctx = new_ctx();
    info(
        ctx.clone(),
        "test",
        r#"特殊字符: @#$%^&*()_+-=[]{}|;':",./<>?"#,
    );
    info(ctx.clone(), "test", r#"JSON: {"key": "value"}"#);
}

#[test]
fn test_empty_message() {
    let ctx = new_ctx();
    info(ctx, "test", "");
}

#[test]
fn test_very_long_message() {
    let ctx = new_ctx();
    let long_msg = "A".repeat(1000);
    info(ctx, "test", &long_msg);
}
