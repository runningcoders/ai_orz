use crate::api::{ApiResponse, AttachmentDetail, AttachmentListQuery};
use crate::enums::FileType;

#[test]
fn attachment_query_and_response_serialize_contract() {
    let query = AttachmentListQuery {
        purpose: Some("skill".to_string()),
        file_type: Some(FileType::Document),
        limit: Some(20),
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
