use super::*;
use crate::error::ErrorCode;

#[test]
fn model_errors_are_identified() {
    for c in [
        ErrorCode::ModelRateLimited,
        ErrorCode::ModelServerError,
        ErrorCode::ModelBadRequest,
        ErrorCode::ModelAuth,
        ErrorCode::ModelContentFiltered,
    ] {
        assert!(
            Error::new(c, "x").is_model_error(),
            "{c:?} should be a model error"
        );
    }
    assert!(!Error::new(ErrorCode::Internal, "x").is_model_error());
}

#[test]
fn only_rate_limit_and_server_are_retryable() {
    assert!(Error::new(ErrorCode::ModelRateLimited, "x").is_retryable());
    assert!(Error::new(ErrorCode::ModelServerError, "x").is_retryable());
    assert!(!Error::new(ErrorCode::ModelBadRequest, "x").is_retryable());
    assert!(!Error::new(ErrorCode::ModelAuth, "x").is_retryable());
    assert!(!Error::new(ErrorCode::ModelContentFiltered, "x").is_retryable());
}

#[test]
fn user_message_uses_code_then_fallback() {
    assert_eq!(
        Error::new(ErrorCode::ModelRateLimited, "detail").user_message(),
        "模型服务请求过于频繁（触发限流），请稍后重试。"
    );
    // 未定义文案的错误码回退到原始详情
    assert_eq!(
        Error::new(ErrorCode::Internal, "boom").user_message(),
        "boom"
    );
}
