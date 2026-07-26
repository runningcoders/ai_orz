//! Project 域 API - 项目管理、任务管理

use common::api::{
    ArtifactDetail, CreateArtifactRequest, CreateProjectRequest, CreateProjectResponse,
    CreateTaskRequest, CreateTaskResponse, GetProjectResponse, GetTaskResponse, ListTasksResponse,
    PagedResult, ProjectListItem, ProjectQueryRequest, TaskListItem, TaskQueryRequest,
    UpdateProjectRequest, UpdateProjectResponse, UpdateTaskRequest, UpdateTaskResponse,
};

use super::{ApiError, api_delete, api_get, api_get_or_default, api_post, api_put, api_put_empty};

// ===== 项目管理 =====

pub async fn list_projects(
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<PagedResult<ProjectListItem>, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit {
        params.push(format!("limit={}", l));
    }
    if let Some(o) = offset {
        params.push(format!("offset={}", o));
    }
    let url = if params.is_empty() {
        "/api/v1/projects".to_string()
    } else {
        format!("/api/v1/projects?{}", params.join("&"))
    };
    api_get(&url).await
}

pub async fn query_projects(
    req: &ProjectQueryRequest,
) -> Result<PagedResult<ProjectListItem>, ApiError> {
    api_post("/api/v1/projects/query", req).await
}

pub async fn get_project(
    id: &str,
    stats_options: Option<&super::StatsOptions>,
) -> Result<GetProjectResponse, ApiError> {
    let url = super::build_url_with_stats(&format!("/api/v1/projects/{}", id), stats_options);
    api_get(&url).await
}

pub async fn create_project(req: CreateProjectRequest) -> Result<CreateProjectResponse, ApiError> {
    api_post("/api/v1/projects", &req).await
}

pub async fn update_project(
    id: &str,
    req: UpdateProjectRequest,
) -> Result<UpdateProjectResponse, ApiError> {
    api_put(&format!("/api/v1/projects/{}", id), &req).await
}

pub async fn update_project_status(id: &str, status: i32) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/projects/{}/status", id), &body).await
}

// ===== 任务管理 =====

pub async fn list_project_tasks(project_id: &str) -> Result<ListTasksResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/projects/{}/tasks", project_id)).await
}

pub async fn list_tasks(
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<PagedResult<TaskListItem>, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit {
        params.push(format!("limit={}", l));
    }
    if let Some(o) = offset {
        params.push(format!("offset={}", o));
    }
    let url = if params.is_empty() {
        "/api/v1/tasks".to_string()
    } else {
        format!("/api/v1/tasks?{}", params.join("&"))
    };
    api_get(&url).await
}

pub async fn query_tasks(req: &TaskQueryRequest) -> Result<PagedResult<TaskListItem>, ApiError> {
    api_post("/api/v1/tasks/query", req).await
}

pub async fn get_task(
    id: &str,
    stats_options: Option<&super::StatsOptions>,
) -> Result<GetTaskResponse, ApiError> {
    let url = super::build_url_with_stats(&format!("/api/v1/tasks/{}", id), stats_options);
    api_get(&url).await
}

pub async fn create_task(req: CreateTaskRequest) -> Result<CreateTaskResponse, ApiError> {
    api_post("/api/v1/tasks", &req).await
}

pub async fn update_task(id: &str, req: UpdateTaskRequest) -> Result<UpdateTaskResponse, ApiError> {
    api_put(&format!("/api/v1/tasks/{}", id), &req).await
}

pub async fn update_task_status(id: &str, status: i32) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/tasks/{}/status", id), &body).await
}

pub async fn update_task_progress(id: &str, progress: i32) -> Result<GetTaskResponse, ApiError> {
    let body = serde_json::json!({ "id": id, "progress": progress });
    api_put(&format!("/api/v1/tasks/{}/progress", id), &body).await
}

// ===== 产物管理 =====

pub async fn list_artifacts(project_id: &str) -> Result<Vec<ArtifactDetail>, ApiError> {
    api_get_or_default(&format!(
        "/api/v1/project/artifacts?project_id={}",
        project_id
    ))
    .await
}

pub async fn create_artifact(req: CreateArtifactRequest) -> Result<ArtifactDetail, ApiError> {
    api_post("/api/v1/project/artifacts", &req).await
}

pub async fn delete_artifact(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/project/artifacts/{}", id)).await
}

// ===== Artifact 内容 =====

pub async fn get_artifact_content(
    id: &str,
) -> Result<common::api::GetArtifactContentResponse, ApiError> {
    api_get(&format!("/api/v1/project/artifacts/{}/content", id)).await
}

pub async fn update_artifact_content(
    id: &str,
    content: String,
) -> Result<ArtifactDetail, ApiError> {
    let req = common::api::UpdateArtifactContentRequest {
        artifact_id: id.to_string(),
        content,
        expected_updated_at: None,
    };
    api_put(&format!("/api/v1/project/artifacts/{}/content", id), &req).await
}
