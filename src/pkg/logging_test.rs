//! 日志模块单元测试（只测试日志格式化输出）

use super::RequestContext;
use crate::pkg::logging::{debug, info, log_error, warn};
use tokio::runtime::Runtime;
use sqlx::sqlite::SqlitePool;

fn create_test_pool() -> SqlitePool {
    crate::config::init().unwrap();
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        crate::pkg::storage::init("sqlite::memory:").await;
        crate::pkg::storage::get().pool_owned()
    })
}

fn new_ctx() -> RequestContext {
    let pool = create_test_pool();
    RequestContext::new_simple("", pool)
}

fn new_ctx_with_user(user_id: &str) -> RequestContext {
    let pool = create_test_pool();
    RequestContext::new_simple(user_id, pool)
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
    info(ctx, "anonymous", "匿名用户访问");
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

#[test]
fn test_multiple_logs_same_context() {
    let ctx = new_ctx_with_user("test_user");

    info(ctx.clone(), "step1", "第一步操作");
    info(ctx.clone(), "step2", "第二步操作");
    info(ctx.clone(), "step3", "第三步操作完成");

    assert_eq!(ctx.log_id.len(), 20);
}
