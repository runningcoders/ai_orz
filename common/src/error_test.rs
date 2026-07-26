//! Contract tests for the shared error model.

use crate::error::{Error, ErrorCode, ErrorField, ErrorType, Result, bail_err, ensure_err, err};
use std::assert_matches;

fn returns_common_result() -> Result<()> {
    bail_err!(
        ToolAutoModeNotSupported,
        "{} Tool only supports Manual control mode",
        "HTTP"
    );
}

#[test]
fn error_code_exposes_stable_metadata() {
    let code = ErrorCode::ToolAutoModeNotSupported;

    assert_eq!(code.code_str(), "tool_auto_mode_not_supported");
    assert_eq!(code.error_type(), ErrorType::Tool);
    assert_eq!(code.http_status(), 400);
}

#[test]
fn err_macro_builds_typed_error() {
    let error = err!(ResourceNotFound, "tool not found: {}", "tool-1");

    assert_eq!(error.code.code_str(), "resource_not_found");
    assert_eq!(error.msg, "tool not found: tool-1");
    assert_matches!(error.code, ErrorCode::ResourceNotFound);
}

#[test]
fn bail_err_macro_returns_common_result() {
    let error = returns_common_result().expect_err("expected typed error");

    assert_matches!(error.code, ErrorCode::ToolAutoModeNotSupported);
    assert_eq!(error.msg, "HTTP Tool only supports Manual control mode");
}

#[test]
fn ensure_err_macro_keeps_success_and_bails_on_failure() {
    fn validate(value: i32) -> Result<i32> {
        ensure_err!(value > 0, InvalidRequest, "value must be positive");
        Ok(value)
    }

    assert_eq!(validate(1).expect("positive value should pass"), 1);

    let error = validate(0).expect_err("zero should fail validation");
    assert_eq!(error.code.code_str(), "invalid_request");
    assert_eq!(error.code.error_type(), ErrorType::Validation);
    assert_eq!(error.code.http_status(), 400);
}

#[test]
fn error_with_field_data_works() {
    let mut field = ErrorField::new();
    field.insert("field_name".into(), "username".into());
    field.insert("reason".into(), "too short".into());

    let error = err!(InvalidRequest, "field validation failed").with_field(field);

    assert_eq!(error.code.code_str(), "invalid_request");
    assert!(error.field.is_some());
    let field = error.field.as_ref().unwrap();
    assert_eq!(
        field.get("field_name").and_then(|v| v.as_str()),
        Some("username")
    );
    assert_eq!(
        field.get("reason").and_then(|v| v.as_str()),
        Some("too short")
    );
}

#[test]
fn error_with_source_keeps_source() {
    let io_err = std::io::Error::other("file not found");
    let error = err!(IoError, "failed to read config").with_source(io_err);

    assert_eq!(error.code.code_str(), "io_error");
    assert!(error.source.is_some());
}
