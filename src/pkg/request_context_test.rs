//! RequestContext 公共方法单元测试

use crate::pkg::request_context::{RequestContext, format_timestamp};
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

#[test]
fn test_request_context() {
    crate::config::init().unwrap();
    // 创建一个内存数据库用于测试
    let pool = create_test_pool();
    let ctx = RequestContext::new_simple("user1", pool);
    assert!(!ctx.log_id.is_empty());
    assert_eq!(ctx.uid(), "user1");
}

#[test]
fn test_log_id_format() {
    let pool = create_test_pool();
    let ctx = RequestContext::new_simple("", pool);
    let log_id = &ctx.log_id;
    assert_eq!(log_id.len(), 20, "log_id 长度应为20位");
    assert!(
        log_id.chars().all(|c: char| c.is_ascii_digit()),
        "log_id 应为纯数字"
    );
}

#[test]
fn test_log_id_uniqueness() {
    let pool = create_test_pool();
    let ctx1 = RequestContext::new_simple("", pool.clone());
    let ctx2 = RequestContext::new_simple("", pool);
    println!("ctx1: {}, ctx2: {}", ctx1.log_id, ctx2.log_id);
    assert_ne!(ctx1.log_id, ctx2.log_id);
}

#[test]
fn test_log_id_from_header() {
    use axum::http::HeaderValue;
    // 初始化 storage
    let _pool = create_test_pool();

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

    // RequestContext::from_headers 内部会调用 storage::get()，已经提前初始化
    let ctx = RequestContext::from_headers(&headers);

    assert_eq!(ctx.log_id, "20260331013000000123");
    assert_eq!(ctx.uid(), "user_001");
    assert_eq!(ctx.uname(), "zhang_san");
}

#[test]
fn test_log_id_auto_generate_when_missing() {
    // 提前初始化 storage 供 from_headers 使用
    let _pool = create_test_pool();
    let headers = axum::http::HeaderMap::new();
    let ctx = RequestContext::from_headers(&headers);

    assert!(!ctx.log_id.is_empty());
    assert_eq!(ctx.log_id.len(), 20);
}

#[test]
fn test_context_uid_helper() {
    let pool = create_test_pool();
    let ctx_with_user = RequestContext::new_simple("test_user", pool.clone());
    assert_eq!(ctx_with_user.uid(), "test_user");

    let ctx_without_user = RequestContext::new_simple("", pool);
    assert_eq!(ctx_without_user.uid(), "");
}

#[test]
fn test_context_uname_helper() {
    // from_headers 测试 username
    use axum::http::HeaderValue;
    let _pool = create_test_pool();

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::HeaderName::from_static("x-username"),
        HeaderValue::from_static("test_user"),
    );
    let ctx = RequestContext::from_headers(&headers);
    assert_eq!(ctx.uname(), "test_user");

    // new_simple 测试 username 为空
    let storage2 = create_test_pool();
    let ctx2 = RequestContext::new_simple("user_id", storage2);
    assert_eq!(ctx2.uname(), "");
}

#[test]
fn test_format_timestamp() {
    use chrono::Utc;
    let timestamp = Utc::now().timestamp() as u64; // 秒级时间戳
    let formatted = format_timestamp(timestamp);
    assert_eq!(formatted.len(), 14, "时间戳格式化后应为14位数字");
    assert!(
        formatted.chars().all(|c| c.is_ascii_digit()),
        "格式化后的时间戳必须全为数字"
    );
}

#[test]
fn test_generate_log_id() {
    // 测试 log_id 生成的格式
    let storage = create_test_pool();
    let ctx = RequestContext::new_simple("", storage);
    let log_id = ctx.log_id;

    // 格式: yyyyMMddHHmmss + 6位随机数 = 20位
    assert_eq!(log_id.len(), 20);

    // 前14位应该是当前时间的格式
    let time_part = &log_id[0..14];
    assert!(time_part.chars().all(|c| c.is_ascii_digit()));

    // 后6位应该是随机数
    let random_part = &log_id[14..20];
    assert!(random_part.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn test_clone_context() {
    let storage = create_test_pool();
    let ctx1 = RequestContext::new_simple("user1", storage);
    let ctx2 = ctx1.clone();

    assert_eq!(ctx1.log_id, ctx2.log_id);
    assert_eq!(ctx1.uid(), ctx2.uid());
    assert_eq!(ctx1.uname(), ctx2.uname());
}

#[test]
fn test_context_agent_id_setter() {
    let storage = create_test_pool();
    let mut ctx = RequestContext::new_simple("user1", storage);
    ctx.set_agent_id("agent_001");
    assert_eq!(ctx.agent_id, Some("agent_001".to_string()));
}

#[test]
fn test_context_task_id_setter() {
    let storage = create_test_pool();
    let mut ctx = RequestContext::new_simple("user1", storage);
    ctx.set_task_id("task_001");
    assert_eq!(ctx.task_id, Some("task_001".to_string()));
}

#[test]
fn test_context_project_id_setter() {
    let storage = create_test_pool();
    let mut ctx = RequestContext::new_simple("user1", storage);
    ctx.set_project_id("project_001");
    assert_eq!(ctx.project_id, Some("project_001".to_string()));
}
