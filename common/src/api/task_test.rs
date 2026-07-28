//! Task API DTO contract tests.

use super::{
    ApiResponse, CreateTaskRequest, GetTaskResponse, TaskListItem, UpdateTaskRequest,
    UpdateTaskStatusRequest, UpdateTaskStatusResponse,
};
use crate::enums::{AssigneeType, TaskStatus};

#[test]
fn task_requests_and_responses_serialize_contract() {
    let create = CreateTaskRequest {
        title: "Task Alpha".to_string(),
        description: Some("Do the first task".to_string()),
        priority: Some(3),
        tags: Some(vec!["alpha".to_string(), "backend".to_string()]),
        root_user_id: Some("user-1".to_string()),
        assignee_type: Some(AssigneeType::Agent),
        assignee_id: "agent-1".to_string(),
        project_id: Some("project-1".to_string()),
        due_at: Some(10),
        dependencies: Some(vec!["task-0".to_string()]),
    };

    let json = serde_json::to_string(&create).unwrap();
    assert!(json.contains("Task Alpha"));
    let decoded: CreateTaskRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.title, "Task Alpha");
    assert_eq!(decoded.assignee_type, Some(AssigneeType::Agent));
    assert_eq!(decoded.dependencies, Some(vec!["task-0".to_string()]));

    let list_item = TaskListItem {
        id: "task-1".to_string(),
        title: "Task Alpha".to_string(),
        description: Some("Do the first task".to_string()),
        status: TaskStatus::Pending as i32,
        priority: 3,
        tags: vec!["alpha".to_string()],
        root_user_id: "user-1".to_string(),
        assignee_type: AssigneeType::Agent as i32,
        assignee_id: "agent-1".to_string(),
        project_id: Some("project-1".to_string()),
        thinking_depth: 0,
        progress: 0,
        created_at: 1,
        updated_at: 2,
        dependencies: vec![],
    };
    let response: ApiResponse<Vec<TaskListItem>> = ApiResponse::success(vec![list_item]);
    assert!(response.is_success());
    assert_eq!(response.data.unwrap()[0].status, TaskStatus::Pending as i32);
}

#[test]
fn update_task_status_request_uses_task_status_enum() {
    let request = UpdateTaskStatusRequest {
        id: "task-1".to_string(),
        status: TaskStatus::InProgress,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(json, r#"{"id":"task-1","status":"InProgress"}"#);

    let decoded: UpdateTaskStatusRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.id, "task-1");
    assert_eq!(decoded.status, TaskStatus::InProgress);
}

#[test]
fn update_task_status_response_uses_task_detail_contract() {
    let response: ApiResponse<UpdateTaskStatusResponse> = ApiResponse::success(GetTaskResponse {
        id: "task-1".to_string(),
        title: "Task Alpha".to_string(),
        description: Some("Do the first task".to_string()),
        status: TaskStatus::Completed as i32,
        priority: 3,
        tags: vec!["alpha".to_string()],
        due_at: Some(10),
        start_at: Some(1),
        end_at: Some(2),
        dependencies: vec!["task-0".to_string()],
        root_user_id: "user-1".to_string(),
        assignee_type: AssigneeType::Agent as i32,
        assignee_id: "agent-1".to_string(),
        project_id: Some("project-1".to_string()),
        thinking_depth: 1,
        progress: 100,
        created_by: "user-1".to_string(),
        modified_by: "user-1".to_string(),
        created_at: 1,
        updated_at: 2,
        stats: None,
        model_call_stats: None,
        artifacts: None,
    });

    assert!(response.is_success());
    let data = response.data.expect("response should contain task detail");
    assert_eq!(data.id, "task-1");
    assert_eq!(data.status, TaskStatus::Completed as i32);
}

#[test]
fn update_task_request_allows_partial_fields() {
    let request = UpdateTaskRequest {
        id: "task-1".to_string(),
        title: Some("Renamed".to_string()),
        description: None,
        priority: None,
        tags: Some(vec!["updated".to_string()]),
        due_at: None,
        dependencies: Some(vec!["task-0".to_string()]),
    };

    let json = serde_json::to_string(&request).unwrap();
    let decoded: UpdateTaskRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.id, "task-1");
    assert_eq!(decoded.title.as_deref(), Some("Renamed"));
    assert_eq!(decoded.tags, Some(vec!["updated".to_string()]));
    assert_eq!(decoded.dependencies, Some(vec!["task-0".to_string()]));
}
