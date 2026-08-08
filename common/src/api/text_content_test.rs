//! Shared text content API DTO contract tests.

use super::{ApiResponse, TextContentResponse, UpdateTextContentRequest};

#[test]
fn text_content_response_serializes_utf8_contract() {
    let content = "# 标题\nhello".to_string();
    let response = TextContentResponse {
        size: content.len() as u64,
        content,
        encoding: "utf-8".to_string(),
        updated_at: 1_718_000_000,
    };

    let api_response = ApiResponse::success(response);
    let json = serde_json::to_string(&api_response).unwrap();

    assert!(json.contains("# 标题"));
    assert!(json.contains("utf-8"));
    assert!(json.contains("updated_at"));

    let decoded: ApiResponse<TextContentResponse> = serde_json::from_str(&json).unwrap();
    let data = decoded.data.unwrap();
    assert_eq!(data.content, "# 标题\nhello");
    assert_eq!(data.encoding, "utf-8");
    assert_eq!(data.size, "# 标题\nhello".len() as u64);
    assert_eq!(data.updated_at, 1_718_000_000);
}

#[test]
fn update_text_content_request_supports_optional_optimistic_lock() {
    let request = UpdateTextContentRequest {
        content: "new content".to_string(),
        expected_updated_at: Some(1_718_000_000),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("new content"));
    assert!(json.contains("expected_updated_at"));

    let decoded: UpdateTextContentRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.content, "new content");
    assert_eq!(decoded.expected_updated_at, Some(1_718_000_000));

    let overwrite_json = r#"{"content":"overwrite"}"#;
    let overwrite: UpdateTextContentRequest = serde_json::from_str(overwrite_json).unwrap();
    assert_eq!(overwrite.content, "overwrite");
    assert_eq!(overwrite.expected_updated_at, None);
}
