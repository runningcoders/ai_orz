//! Common API response assertions.

use axum::http::StatusCode;
use serde_json::Value;

/// Assert that the response is `200 OK` with `code: 0` in the API envelope.
/// Returns the `data` field for further assertions.
pub fn assert_api_ok(status: StatusCode, body: &Value) -> Value {
    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200 OK, got {}: {}",
        status,
        body
    );
    let code = body
        .get("code")
        .unwrap_or_else(|| panic!("response missing 'code' field: {}", body))
        .as_i64()
        .unwrap_or_else(|| panic!("'code' field is not an integer: {}", body));
    assert_eq!(
        code, 0,
        "expected code=0 (success), got code={}: {}",
        code, body
    );
    body.get("data")
        .cloned()
        .unwrap_or_else(|| panic!("response missing 'data' field: {}", body))
}

/// Assert that the response has the given HTTP status and a non-zero `code` in the envelope.
pub fn assert_api_error(status: StatusCode, body: &Value, expected_status: StatusCode) {
    assert_eq!(
        status, expected_status,
        "expected {} got {}: {}",
        expected_status, status, body
    );
    let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    assert!(
        code != 0,
        "expected non-zero error code, got code=0 with body: {}",
        body
    );
}
