//! 日志模块单元测试（只测试日志格式化输出）

use super::RequestContext;
use sqlx::sqlite::SqlitePool;
use tokio::runtime::Runtime;

fn create_test_pool() -> SqlitePool {
    crate::config::init().unwrap();
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        crate::pkg::storage::init_for_test().await;
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
    log_info!(&new_ctx(), "test_info", "这是一条 info 日志");
}

#[test]
fn test_warn_log() {
    log_warn!(&new_ctx(), "test_warn", "这是一条 warn 日志");
}

#[test]
fn test_error_log() {
    log_error!(&new_ctx(), "test_error", "这是一条 error 日志");
}

#[test]
fn test_debug_log() {
    log_debug!(&new_ctx(), "test_debug", "这是一条 debug 日志");
}

#[test]
fn test_log_with_empty_user() {
    log_info!(&new_ctx(), "anonymous", "匿名用户访问");
}

#[test]
fn test_long_operation_name() {
    log_info!(
        &new_ctx(),
        "create_agent_with_validation",
        "创建 Agent 并验证"
    );
}

#[test]
fn test_chinese_message() {
    let ctx = new_ctx();
    log_info!(&ctx, "test", "这是一条中文测试日志消息");
    log_error!(&ctx, "test", "错误信息：数据库连接失败");
    log_warn!(&ctx, "test", "警告：内存使用率超过 80%");
}

#[test]
fn test_special_characters_in_message() {
    let ctx = new_ctx();
    // 注意：% 和 {} 在 tracing 宏中有特殊含义，需要转义
    log_info!(&ctx, "test", "特殊字符: @#$%^&*()_+-=[]{{}}|;':\",./<>?",);
    log_info!(&ctx, "test", "JSON: {{\"key\": \"value\"}}");
}

#[test]
fn test_empty_message() {
    log_info!(&new_ctx(), "test", "");
}

#[test]
fn test_very_long_message() {
    let long_msg = "A".repeat(1000);
    log_info!(&new_ctx(), "test", "{}", long_msg);
}

#[test]
fn test_multiple_logs_same_context() {
    let ctx = new_ctx_with_user("test_user");

    log_info!(&ctx, "step1", "第一步操作");
    log_info!(&ctx, "step2", "第二步操作");
    log_info!(&ctx, "step3", "第三步操作完成");

    assert_eq!(ctx.log_id.len(), 20);
}

#[test]
fn test_structured_log_fields() {
    let ctx = new_ctx();
    // 测试宏支持的结构化字段语法
    log_debug!(&ctx, "cache", key = %"user:1001", "缓存命中");
    log_warn!(&ctx, "db", error = ?"connection timeout", "查询超时");
}

#[test]
fn test_format_placeholders() {
    let ctx = new_ctx();
    // 测试各种格式化占位符
    log_info!(&ctx, "format", "字符串: {}, 数字: {}", "hello", 42);
    log_debug!(&ctx, "format", "调试: {:?}", vec![1, 2, 3]);
}
