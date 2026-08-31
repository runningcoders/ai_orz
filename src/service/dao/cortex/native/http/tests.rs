use super::*;
use common::error::ErrorCode;
use reqwest::StatusCode;

fn sc(u: u16) -> StatusCode {
    StatusCode::from_u16(u).unwrap()
}

#[test]
fn classifies_rate_limit_as_retryable() {
    let e = model_call_error("chat", sc(429), "slow down");
    assert!(e.is_model_error());
    assert!(e.is_retryable());
    assert!(matches!(e.code, ErrorCode::ModelRateLimited));
}

#[test]
fn classifies_5xx_as_server_retryable() {
    for s in [500u16, 502, 503, 599] {
        let e = model_call_error("chat", sc(s), "boom");
        assert!(e.is_retryable(), "status {s} should be retryable");
        assert!(
            matches!(e.code, ErrorCode::ModelServerError),
            "status {s} -> server error"
        );
    }
}

#[test]
fn classifies_auth_as_non_retryable() {
    for s in [401u16, 403] {
        let e = model_call_error("chat", sc(s), "no auth");
        assert!(!e.is_retryable(), "status {s} should NOT be retryable");
        assert!(matches!(e.code, ErrorCode::ModelAuth));
    }
}

#[test]
fn classifies_4xx_as_bad_request_unless_content_filtered() {
    let bad = model_call_error("chat", sc(400), "invalid param");
    assert!(matches!(bad.code, ErrorCode::ModelBadRequest));
    assert!(!bad.is_retryable());

    let filtered = model_call_error("chat", sc(400), "error: content_filter triggered");
    assert!(matches!(filtered.code, ErrorCode::ModelContentFiltered));
    assert!(!filtered.is_retryable());

    let moderation = model_call_error("chat", sc(422), "{\"type\":\"content_filter\"}");
    assert!(matches!(moderation.code, ErrorCode::ModelContentFiltered));
}
