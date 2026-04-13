//! RequestContext 模块单元测试

use super::RequestContext;
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

#[test]
fn test_log_id_format() {
    let _pool = create_test_pool();
    let ctx = RequestContext::new_simple("", _pool);
    let log_id = &ctx.log_id;
    assert_eq!(log_id.len(), 20, "log_id 长度应为20位");
    assert!(
        log_id.chars().all(|c: char| c.is_ascii_digit()),
        "log_id 应为纯数字"
    );
}

#[test]
fn test_log_id_uniqueness() {
    let _pool = create_test_pool();
    let ctx1 = RequestContext::new_simple("", _pool.clone());
    let ctx2 = RequestContext::new_simple("", _pool);
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
        axum::http::header::HeaderName::from_static("x-user-id"),
        HeaderValue::from_static("test_user"),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("x-username"),
        HeaderValue::from_static("test_name"),
    );

    let ctx = RequestContext::from_headers(&headers);
    assert_eq!(ctx.uname(), "test_name");

    let headers_empty = axum::http::HeaderMap::new();
    let ctx_empty = RequestContext::from_headers(&headers_empty);
    assert_eq!(ctx_empty.uname(), "");
}

#[test]
fn test_context_clone() {
    let pool = create_test_pool();
    let ctx1 = RequestContext::new_simple("test_user", pool);
    let ctx2 = ctx1.clone();

    assert_eq!(ctx1.log_id, ctx2.log_id);
    assert_eq!(ctx1.uid(), ctx2.uid());
    assert_eq!(ctx1.uname(), ctx2.uname());
}
