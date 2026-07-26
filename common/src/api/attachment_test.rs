use crate::api::PaginationParams;
use crate::api::{
    ApiResponse, AttachmentDetail, AttachmentListQuery, CreateTextAttachmentRequest,
    TextContentResponse, UpdateTextContentRequest,
};
use crate::enums::FileType;

#[test]
fn attachment_query_and_response_serialize_contract() {
    let query = AttachmentListQuery {
        purpose: Some("skill".to_string()),
        file_type: Some(FileType::Document),
        pagination: PaginationParams {
            limit: Some(20),
            offset: Some(0),
        },
    };
    let query_json = serde_json::to_string(&query).unwrap();
    assert!(query_json.contains("skill"));

    let detail = AttachmentDetail {
        id: "att-1".to_string(),
        original_name: "skill.md".to_string(),
        stored_name: "att-1.md".to_string(),
        relative_path: "20260617/att-1.md".to_string(),
        mime_type: "text/markdown".to_string(),
        file_type: FileType::Document,
        size: 12,
        purpose: "skill".to_string(),
        root_user_id: "user-1".to_string(),
        created_by: "user-1".to_string(),
        created_at: 1,
        updated_at: 2,
    };
    let response = ApiResponse::success(detail);
    let response_json = serde_json::to_string(&response).unwrap();
    assert!(response_json.contains("att-1"));
    assert!(response_json.contains("skill.md"));
}

#[test]
fn text_attachment_create_and_content_contract() {
    let request = CreateTextAttachmentRequest {
        file_name: "notes.md".to_string(),
        content: "# Notes".to_string(),
        mime_type: None,
        purpose: Some("skill".to_string()),
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("notes.md"));
    assert!(json.contains("# Notes"));

    let decoded: CreateTextAttachmentRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.file_name, "notes.md");
    assert_eq!(decoded.mime_type, None);
    assert_eq!(decoded.purpose, Some("skill".to_string()));

    let content = TextContentResponse {
        content: "updated".to_string(),
        encoding: "utf-8".to_string(),
        size: 7,
        updated_at: 42,
    };
    let response = ApiResponse::success(content);
    let response_json = serde_json::to_string(&response).unwrap();
    assert!(response_json.contains("utf-8"));
    assert!(response_json.contains("updated"));

    let update = UpdateTextContentRequest {
        content: "next".to_string(),
        expected_updated_at: Some(42),
    };
    let update_json = serde_json::to_string(&update).unwrap();
    assert!(update_json.contains("expected_updated_at"));
}
