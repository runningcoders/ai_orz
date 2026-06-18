//! Artifact API DTO contract tests.

use super::{
    ApiResponse, ArtifactDetail, CreateArtifactRequest, GetArtifactContentResponse,
    ListArtifactsRequest, UpdateArtifactContentRequest,
};
use crate::enums::{ArtifactSourceType, FileType};

#[test]
fn create_artifact_request_supports_attachment_source_contract() {
    let request = CreateArtifactRequest {
        project_id: "project-1".to_string(),
        task_id: Some("task-1".to_string()),
        name: "Design Doc".to_string(),
        description: Some("Initial design".to_string()),
        source_type: ArtifactSourceType::Attachment,
        attachment_id: Some("attachment-1".to_string()),
        content: None,
        file_name: None,
        mime_type: None,
        file_type: Some(FileType::Document),
        tags: Some(vec!["design".to_string()]),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("attachment"));
    assert!(json.contains("attachment-1"));

    let decoded: CreateArtifactRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.source_type, ArtifactSourceType::Attachment);
    assert_eq!(decoded.attachment_id, Some("attachment-1".to_string()));
}

#[test]
fn create_artifact_request_supports_generated_content_source_contract() {
    let request = CreateArtifactRequest {
        project_id: "project-1".to_string(),
        task_id: None,
        name: "Execution Plan".to_string(),
        description: None,
        source_type: ArtifactSourceType::GeneratedContent,
        attachment_id: None,
        content: Some("# Plan".to_string()),
        file_name: Some("plan.md".to_string()),
        mime_type: Some("text/markdown".to_string()),
        file_type: Some(FileType::Document),
        tags: Some(vec!["plan".to_string()]),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("generated_content"));
    assert!(json.contains("plan.md"));

    let decoded: CreateArtifactRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.source_type, ArtifactSourceType::GeneratedContent);
    assert_eq!(decoded.content, Some("# Plan".to_string()));
}

#[test]
fn artifact_source_type_keeps_remote_url_reserved_for_extension() {
    assert_eq!(
        ArtifactSourceType::from_i32(1),
        ArtifactSourceType::Attachment
    );
    assert_eq!(
        ArtifactSourceType::from_i32(2),
        ArtifactSourceType::GeneratedContent
    );
    assert_eq!(
        ArtifactSourceType::from_i32(3),
        ArtifactSourceType::RemoteUrl
    );
    assert_eq!(ArtifactSourceType::RemoteUrl.to_i32(), 3);
}

#[test]
fn list_artifacts_query_and_response_serialize_contract() {
    let query = ListArtifactsRequest {
        project_id: "project-1".to_string(),
        task_id: None,
        file_type: Some(FileType::Document),
        source_type: Some(ArtifactSourceType::Attachment),
        limit: Some(20),
    };
    let query_json = serde_json::to_string(&query).unwrap();
    assert!(query_json.contains("project-1"));
    assert!(query_json.contains("attachment"));

    let detail = ArtifactDetail {
        id: "artifact-1".to_string(),
        project_id: "project-1".to_string(),
        task_id: None,
        name: "Design Doc".to_string(),
        description: "Initial design".to_string(),
        file_type: FileType::Document,
        source_type: ArtifactSourceType::Attachment,
        file_path: "attachments/20260617/doc.md".to_string(),
        mime_type: "text/markdown".to_string(),
        file_size: 128,
        tags: vec!["design".to_string()],
        status: 1,
        created_by: "user-1".to_string(),
        modified_by: "user-1".to_string(),
        created_at: 1,
        updated_at: 2,
    };
    let response: ApiResponse<ArtifactDetail> = ApiResponse::success(detail);
    let response_json = serde_json::to_string(&response).unwrap();
    assert!(response_json.contains("artifact-1"));
    assert!(response_json.contains("attachments/20260617/doc.md"));
}

#[test]
fn get_artifact_content_response_contract_serializes_correctly() {
    let content = super::ArtifactContentText {
        content: "# Hello World".to_string(),
        encoding: "utf-8".to_string(),
        size: 14,
        updated_at: 1718000000,
    };
    let artifact = ArtifactDetail {
        id: "artifact-1".to_string(),
        project_id: "project-1".to_string(),
        task_id: None,
        name: "Hello.md".to_string(),
        description: "".to_string(),
        file_type: FileType::Document,
        source_type: ArtifactSourceType::GeneratedContent,
        file_path: "artifacts/projects/project-1/artifact-1/Hello.md".to_string(),
        mime_type: "text/markdown".to_string(),
        file_size: 14,
        tags: vec![],
        status: 1,
        created_by: "user-1".to_string(),
        modified_by: "user-1".to_string(),
        created_at: 1718000000,
        updated_at: 1718000000,
    };
    let resp = GetArtifactContentResponse { artifact, content };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("utf-8"));
    assert!(json.contains("Hello World"));
    let decoded: GetArtifactContentResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.content.content, "# Hello World");
    assert_eq!(decoded.content.encoding, "utf-8");
}

#[test]
fn update_artifact_content_request_supports_optional_optimistic_lock() {
    // without optimistic lock
    let req = UpdateArtifactContentRequest {
        artifact_id: "artifact-1".to_string(),
        content: "new content".to_string(),
        expected_updated_at: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    let decoded: UpdateArtifactContentRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.artifact_id, "artifact-1");
    assert_eq!(decoded.content, "new content");
    assert_eq!(decoded.expected_updated_at, None);

    // with optimistic lock
    let req = UpdateArtifactContentRequest {
        artifact_id: "artifact-1".to_string(),
        content: "updated".to_string(),
        expected_updated_at: Some(1718000000),
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("1718000000"));
    let decoded: UpdateArtifactContentRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.artifact_id, "artifact-1");
    assert_eq!(decoded.expected_updated_at, Some(1718000000));
}
