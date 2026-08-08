//! Project API DTO contract tests.

use super::{
    ApiResponse, CreateProjectRequest, GetProjectResponse, ProjectListItem, UpdateProjectRequest,
    UpdateProjectStatusRequest, UpdateProjectStatusResponse,
};
use crate::enums::ProjectStatus;

#[test]
fn project_requests_and_responses_serialize_contract() {
    let create = CreateProjectRequest {
        name: "Project Alpha".to_string(),
        description: Some("Build the first project".to_string()),
        priority: Some(3),
        tags: Some(vec!["alpha".to_string(), "backend".to_string()]),
        owner_agent_id: None,
    };

    let json = serde_json::to_string(&create).unwrap();
    assert!(json.contains("Project Alpha"));
    let decoded: CreateProjectRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.name, "Project Alpha");
    assert_eq!(decoded.priority, Some(3));

    let list_item = ProjectListItem {
        id: "project-1".to_string(),
        name: "Project Alpha".to_string(),
        description: Some("Build the first project".to_string()),
        status: ProjectStatus::Active as i32,
        priority: 3,
        tags: vec!["alpha".to_string()],
        root_user_id: "user-1".to_string(),
        owner_agent_id: None,
        created_at: 1,
        updated_at: 2,
    };
    let response: ApiResponse<Vec<ProjectListItem>> = ApiResponse::success(vec![list_item]);
    assert!(response.is_success());
    assert_eq!(
        response.data.unwrap()[0].status,
        ProjectStatus::Active as i32
    );
}

#[test]
fn update_project_status_request_uses_project_status_enum() {
    let request = UpdateProjectStatusRequest {
        id: "project-1".to_string(),
        status: ProjectStatus::InProgress,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(json, r#"{"id":"project-1","status":"InProgress"}"#);

    let decoded: UpdateProjectStatusRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.id, "project-1");
    assert_eq!(decoded.status, ProjectStatus::InProgress);
}

#[test]
fn update_project_status_response_uses_project_detail_contract() {
    let response: ApiResponse<UpdateProjectStatusResponse> =
        ApiResponse::success(GetProjectResponse {
            id: "project-1".to_string(),
            name: "Project Alpha".to_string(),
            description: Some("Build the first project".to_string()),
            workflow: None,
            guidance: None,
            status: ProjectStatus::Completed as i32,
            priority: 3,
            tags: vec!["alpha".to_string()],
            root_user_id: "user-1".to_string(),
            owner_agent_id: None,
            start_at: Some(1),
            due_at: None,
            end_at: Some(2),
            created_at: 1,
            updated_at: 2,
            stats: None,
            model_call_stats: None,
            task_graph: None,
            execution_plan: None,
            execution_result: None,
            artifacts: None,
            progress_summary: None,
        });

    assert!(response.is_success());
    let data = response
        .data
        .expect("response should contain project detail");
    assert_eq!(data.id, "project-1");
    assert_eq!(data.status, ProjectStatus::Completed as i32);
}

#[test]
fn update_project_request_allows_partial_fields() {
    let request = UpdateProjectRequest {
        id: "project-1".to_string(),
        name: Some("Renamed".to_string()),
        description: None,
        priority: None,
        tags: Some(vec!["updated".to_string()]),
        execution_plan: None,
        execution_result: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    let decoded: UpdateProjectRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.id, "project-1");
    assert_eq!(decoded.name.as_deref(), Some("Renamed"));
    assert_eq!(decoded.tags, Some(vec!["updated".to_string()]));
}
