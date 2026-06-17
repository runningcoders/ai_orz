//! Skill API DTO contract tests.

use super::{
    ApiResponse, CreateSkillRequest, InstallSkillToAgentResponse, SkillDetail, SkillFileInput,
    SkillFileItem, SkillListItem, SkillSearchQuery, UpdateSkillFileContentRequest,
    UpdateSkillRequest,
};
use crate::enums::SkillStatus;
use crate::enums::skill::SkillAuthorType;
use std::collections::HashMap;

#[test]
fn skill_requests_and_responses_serialize_contract() {
    let create = CreateSkillRequest {
        name: "Rust Debugging".to_string(),
        description: "Use for systematic Rust debugging".to_string(),
        tags: vec!["rust".to_string(), "debugging".to_string()],
        category: Some("engineering".to_string()),
        status: Some(SkillStatus::Draft),
        content: Some("# Rust Debugging".to_string()),
        initial_files: None,
    };

    let json = serde_json::to_string(&create).unwrap();
    assert!(json.contains("Rust Debugging"));
    let decoded: CreateSkillRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.name, "Rust Debugging");
    assert_eq!(decoded.status, Some(SkillStatus::Draft));

    let list_item = SkillListItem {
        id: "skill-1".to_string(),
        name: "Rust Debugging".to_string(),
        description: "Use for systematic Rust debugging".to_string(),
        tags: vec!["rust".to_string()],
        category: "engineering".to_string(),
        parent_skill_id: String::new(),
        author_id: "user-1".to_string(),
        author_type: SkillAuthorType::User,
        status: SkillStatus::Published,
        created_at: 1,
        updated_at: 2,
    };
    let response: ApiResponse<Vec<SkillListItem>> = ApiResponse::success(vec![list_item]);
    assert!(response.is_success());
    assert_eq!(response.data.unwrap()[0].status, SkillStatus::Published);
}

#[test]
fn skill_detail_contains_content_but_list_item_does_not() {
    let detail = SkillDetail {
        id: "skill-1".to_string(),
        name: "Rust Debugging".to_string(),
        description: "Use for systematic Rust debugging".to_string(),
        tags: vec!["rust".to_string()],
        category: "engineering".to_string(),
        parent_skill_id: String::new(),
        author_id: "user-1".to_string(),
        author_type: SkillAuthorType::User,
        modifier_id: "user-1".to_string(),
        status: SkillStatus::Published,
        content: Some("# Rust Debugging".to_string()),
        files: vec![SkillFileItem {
            filename: "skill.md".to_string(),
            file_size: 16,
            has_content: true,
        }],
        created_at: 1,
        updated_at: 2,
    };

    let response: ApiResponse<SkillDetail> = ApiResponse::success(detail);
    let data = response.data.expect("response should contain skill detail");
    assert_eq!(data.content.as_deref(), Some("# Rust Debugging"));
    assert_eq!(data.files[0].filename, "skill.md");
}

#[test]
fn update_skill_request_allows_partial_fields() {
    let request = UpdateSkillRequest {
        name: Some("Renamed".to_string()),
        description: None,
        tags: Some(vec!["updated".to_string()]),
        category: None,
        status: Some(SkillStatus::Published),
        content: None,
        files: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    let decoded: UpdateSkillRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.name.as_deref(), Some("Renamed"));
    assert_eq!(decoded.tags, Some(vec!["updated".to_string()]));
    assert_eq!(decoded.status, Some(SkillStatus::Published));
}

#[test]
fn update_skill_request_accepts_attachment_file_imports() {
    let request = UpdateSkillRequest {
        files: Some(vec![SkillFileInput {
            attachment_id: "attachment-1".to_string(),
            target_path: "references/guide.md".to_string(),
        }]),
        ..Default::default()
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("attachment-1"));
    assert!(json.contains("references/guide.md"));

    let decoded: UpdateSkillRequest = serde_json::from_str(&json).unwrap();
    let files = decoded.files.expect("files should deserialize");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].attachment_id, "attachment-1");
    assert_eq!(files[0].target_path, "references/guide.md");
}

#[test]
fn skill_search_query_uses_skill_status_enum() {
    let query = SkillSearchQuery {
        keyword: Some("debug".to_string()),
        status: Some(SkillStatus::Published),
        category: Some("engineering".to_string()),
        limit: Some(10),
    };

    let json = serde_json::to_string(&query).unwrap();
    assert!(json.contains("Published"));
    let decoded: SkillSearchQuery = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.status, Some(SkillStatus::Published));
}

#[test]
fn install_skill_to_agent_response_returns_installed_skill_detail() {
    let response: ApiResponse<InstallSkillToAgentResponse> =
        ApiResponse::success(InstallSkillToAgentResponse {
            agent_id: "agent-1".to_string(),
            source_skill_id: "skill-source".to_string(),
            skill: SkillDetail {
                id: "skill-installed".to_string(),
                name: "Rust Debugging".to_string(),
                description: "Use for systematic Rust debugging".to_string(),
                tags: vec!["rust".to_string()],
                category: "engineering".to_string(),
                parent_skill_id: "skill-source".to_string(),
                author_id: "agent-1".to_string(),
                author_type: SkillAuthorType::Agent,
                modifier_id: "agent-1".to_string(),
                status: SkillStatus::Draft,
                content: Some("# Rust Debugging".to_string()),
                files: vec![],
                created_at: 1,
                updated_at: 2,
            },
        });

    assert!(response.is_success());
    let data = response
        .data
        .expect("response should contain installed skill");
    assert_eq!(data.agent_id, "agent-1");
    assert_eq!(data.skill.parent_skill_id, "skill-source");
    assert_eq!(data.skill.status, SkillStatus::Draft);
}

#[test]
fn create_skill_request_accepts_initial_files() {
    let mut files = HashMap::new();
    files.insert("skill.md".to_string(), "# Main Content".to_string());
    files.insert("prompt.md".to_string(), "System Prompt".to_string());

    let request = CreateSkillRequest {
        name: "Multi-file Skill".to_string(),
        description: "Skill with multiple markdown files".to_string(),
        tags: vec!["template".to_string()],
        category: Some("prompt".to_string()),
        status: Some(SkillStatus::Draft),
        content: None,
        initial_files: Some(files),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("skill.md"));
    assert!(json.contains("Main Content"));
    assert!(json.contains("prompt.md"));

    let decoded: CreateSkillRequest = serde_json::from_str(&json).unwrap();
    let decoded_files = decoded
        .initial_files
        .expect("initial_files should deserialize");
    assert_eq!(decoded_files.len(), 2);
    assert_eq!(
        decoded_files.get("skill.md"),
        Some(&"# Main Content".to_string())
    );
    assert_eq!(
        decoded_files.get("prompt.md"),
        Some(&"System Prompt".to_string())
    );
}

#[test]
fn update_skill_file_content_request_supports_optimistic_lock() {
    let request = UpdateSkillFileContentRequest {
        content: "Updated content".to_string(),
        expected_updated_at: Some(1234567890),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("Updated content"));
    assert!(json.contains("1234567890"));

    let decoded: UpdateSkillFileContentRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.content, "Updated content");
    assert_eq!(decoded.expected_updated_at, Some(1234567890));
}
