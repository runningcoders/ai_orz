//! Skill API DTO contract tests.

use super::{
    ApiResponse, CreateSkillRequest, InstallSkillToAgentResponse, SKILL_MAIN_FILE,
    SkillContentInput, SkillDetail, SkillFileInput, SkillFileItem, SkillListItem, SkillSearchQuery,
    UpdateSkillFileContentRequest, UpdateSkillRequest, default_target_path,
};
use crate::enums::SkillStatus;
use crate::enums::skill::SkillAuthorType;

#[test]
fn skill_requests_and_responses_serialize_contract() {
    let create = CreateSkillRequest {
        name: "Rust Debugging".to_string(),
        description: "Use for systematic Rust debugging".to_string(),
        tags: vec!["rust".to_string(), "debugging".to_string()],
        category: Some("engineering".to_string()),
        status: Some(SkillStatus::Draft),
        content_input: Some(SkillContentInput {
            content: Some("# Rust Debugging".to_string()),
            ..Default::default()
        }),
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
            is_main: true,
            abs_path: "/data/skills/skill-1/skill.md".to_string(),
            executable: false,
        }],
        created_at: 1,
        updated_at: 2,
        can_publish: false,
    };

    let response: ApiResponse<SkillDetail> = ApiResponse::success(detail);
    let data = response.data.expect("response should contain skill detail");
    assert_eq!(data.content.as_deref(), Some("# Rust Debugging"));
    assert_eq!(data.files[0].filename, "skill.md");
}

#[test]
fn update_skill_request_allows_partial_fields() {
    let request = UpdateSkillRequest {
        skill_id: "skill-1".to_string(),
        name: Some("Renamed".to_string()),
        description: None,
        tags: Some(vec!["updated".to_string()]),
        category: None,
        status: Some(SkillStatus::Published),
        content_input: None,
        file_deletes: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    let decoded: UpdateSkillRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.skill_id, "skill-1");
    assert_eq!(decoded.name.as_deref(), Some("Renamed"));
    assert_eq!(decoded.tags, Some(vec!["updated".to_string()]));
    assert_eq!(decoded.status, Some(SkillStatus::Published));
}

#[test]
fn update_skill_request_accepts_attachment_file_imports() {
    let request = UpdateSkillRequest {
        content_input: Some(SkillContentInput {
            files: Some(vec![SkillFileInput {
                attachment_id: Some("attachment-1".to_string()),
                artifact_id: None,
                target_path: "references/guide.md".to_string(),
            }]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("attachment-1"));
    assert!(json.contains("references/guide.md"));

    let decoded: UpdateSkillRequest = serde_json::from_str(&json).unwrap();
    let files = decoded
        .content_input
        .expect("content_input should deserialize")
        .files
        .expect("files should deserialize");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].attachment_id.as_deref(), Some("attachment-1"));
    assert_eq!(files[0].artifact_id, None);
    assert_eq!(files[0].target_path, "references/guide.md");
}

#[test]
fn skill_search_query_uses_skill_status_enum() {
    let query = SkillSearchQuery {
        keyword: Some("debug".to_string()),
        author_id: Some("user-1".to_string()),
        status: Some(SkillStatus::Published),
        category: Some("engineering".to_string()),
        limit: Some(10),
    };

    let json = serde_json::to_string(&query).unwrap();
    assert!(json.contains("Published"));
    let decoded: SkillSearchQuery = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.author_id, Some("user-1".to_string()));
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
                can_publish: false,
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
fn create_skill_request_accepts_content_input_files() {
    let request = CreateSkillRequest {
        name: "Multi-file Skill".to_string(),
        description: "Skill with multiple markdown files".to_string(),
        tags: vec!["template".to_string()],
        category: Some("prompt".to_string()),
        status: Some(SkillStatus::Draft),
        content_input: Some(SkillContentInput {
            content: Some("# Main Content".to_string()),
            files: Some(vec![SkillFileInput {
                attachment_id: Some("att-1".to_string()),
                artifact_id: None,
                target_path: "prompt.md".to_string(),
            }]),
            ..Default::default()
        }),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("Main Content"));
    assert!(json.contains("prompt.md"));

    let decoded: CreateSkillRequest = serde_json::from_str(&json).unwrap();
    let ci = decoded
        .content_input
        .expect("content_input should deserialize");
    assert_eq!(ci.content.as_deref(), Some("# Main Content"));
    let files = ci.files.expect("files should deserialize");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].target_path, "prompt.md");
}

#[test]
fn update_skill_file_content_request_supports_optimistic_lock() {
    let request = UpdateSkillFileContentRequest {
        skill_id: "skill-1".to_string(),
        filename: "skill.md".to_string(),
        content: "Updated content".to_string(),
        expected_updated_at: Some(1234567890),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("Updated content"));
    assert!(json.contains("1234567890"));

    let decoded: UpdateSkillFileContentRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.skill_id, "skill-1");
    assert_eq!(decoded.filename, "skill.md");
    assert_eq!(decoded.content, "Updated content");
    assert_eq!(decoded.expected_updated_at, Some(1234567890));
}

/// 附件默认归位规则（前端上传预填 + 后端导入推断共用同一份 SSOT）。
#[test]
fn default_target_path_organizes_by_extension() {
    assert_eq!(default_target_path("logo.png"), "assets/logo.png");
    assert_eq!(default_target_path("a.JPG"), "assets/a.JPG");
    assert_eq!(default_target_path("deploy.sh"), "scripts/deploy.sh");
    assert_eq!(default_target_path("run.py"), "scripts/run.py");
    // Markdown 一律归位到根引导文件（技能只有一个主文件）
    assert_eq!(default_target_path("guide.md"), SKILL_MAIN_FILE);
    assert_eq!(default_target_path("README.md"), SKILL_MAIN_FILE);
    // 其他类型保持原名平铺
    assert_eq!(default_target_path("data.json"), "data.json");
    // 无扩展名不崩、不误归位
    assert_eq!(default_target_path("Makefile"), "Makefile");
}

/// 文件条目来源必须「有且仅有一个」：attachment_id 与 artifact_id 二选一。
#[test]
fn skill_file_source_is_exclusive_and_required() {
    let input = |att: Option<&str>, art: Option<&str>| SkillContentInput {
        files: Some(vec![SkillFileInput {
            attachment_id: att.map(|s| s.to_string()),
            artifact_id: art.map(|s| s.to_string()),
            target_path: "assets/x.png".to_string(),
        }]),
        ..Default::default()
    };

    // 两个都给：无法确定该读哪一份
    let err = input(Some("att-1"), Some("art-1"))
        .validate_files_single_source()
        .unwrap_err();
    assert!(err.contains("二选一"), "互斥须明确提示二选一: {err}");

    // 两个都空：不是「没传文件」，而是这个文件压根没说清来源
    let err = input(None, None)
        .validate_files_single_source()
        .unwrap_err();
    assert!(err.contains("缺少来源"), "应提示缺少来源: {err}");

    // 空串 / 纯空白视同未提供，不能当作有效来源蒙混过关
    let err = input(Some("   "), None)
        .validate_files_single_source()
        .unwrap_err();
    assert!(err.contains("缺少来源"), "空白 id 应视同未提供: {err}");

    // 二选一各自成立
    assert!(
        input(Some("att-1"), None)
            .validate_files_single_source()
            .is_ok()
    );
    assert!(
        input(None, Some("art-1"))
            .validate_files_single_source()
            .is_ok()
    );

    // 新校验必须并入 validate_all，否则 handler 只调用 validate_all 时会漏掉
    assert!(input(Some("att-1"), Some("art-1")).validate_all().is_err());
    assert!(input(None, None).validate_all().is_err());
    assert!(input(None, Some("art-1")).validate_all().is_ok());
}

/// 向后兼容：存量调用方只发 attachment_id（没有 artifact_id 字段），必须仍能解析。
///
/// `attachment_id` 由 `String` 改 `Option<String>` 是本次最大的断裂风险面，
/// 这条用例钉住「老 payload 不被打挂」。
#[test]
fn legacy_file_payload_without_artifact_id_still_deserializes() {
    let legacy = r#"{"attachment_id":"att-legacy","target_path":"assets/logo.png"}"#;
    let decoded: SkillFileInput = serde_json::from_str(legacy).unwrap();
    assert_eq!(decoded.attachment_id.as_deref(), Some("att-legacy"));
    assert_eq!(decoded.artifact_id, None);
    assert!(decoded.has_attachment());
}

/// 新通路：只给 artifact_id（任务中生成的技能包）可正常往返。
#[test]
fn artifact_only_file_input_roundtrips() {
    let file = SkillFileInput {
        attachment_id: None,
        artifact_id: Some("art-1".to_string()),
        target_path: "skill.md".to_string(),
    };
    let json = serde_json::to_string(&file).unwrap();
    assert!(json.contains("art-1"), "序列化应带上 artifact_id: {json}");

    let back: SkillFileInput = serde_json::from_str(&json).unwrap();
    assert_eq!(back.artifact_id.as_deref(), Some("art-1"));
    assert!(back.has_artifact());
    assert!(!back.has_attachment());
}
