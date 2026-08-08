//! Project 域 API - 项目管理、任务管理

use common::api::{
    ArtifactDetail, CreateArtifactRequest, CreateProjectRequest, CreateProjectResponse,
    CreateTaskRequest, CreateTaskResponse, GetProjectRequest, GetProjectResponse, GetTaskRequest,
    GetTaskResponse, ListProjectsRequest, ListTasksRequest, ListTasksResponse, PagedResult,
    ProjectListItem, ProjectQueryRequest, SearchProjectsRequest, SearchTasksRequest, TaskListItem,
    TaskQueryRequest, UpdateProjectRequest, UpdateProjectResponse, UpdateProjectStatusRequest,
    UpdateTaskProgressRequest, UpdateTaskRequest, UpdateTaskResponse, UpdateTaskStatusRequest,
};

use super::{ApiError, api_delete, api_get, api_get_or_default, api_post, api_put, api_put_empty};

// ===== 项目管理 =====

pub async fn list_projects(
    req: ListProjectsRequest,
) -> Result<PagedResult<ProjectListItem>, ApiError> {
    let url = super::build_pagination_url("/api/v1/projects", &req.pagination);
    api_get(&url).await
}

pub async fn query_projects(
    req: &ProjectQueryRequest,
) -> Result<PagedResult<ProjectListItem>, ApiError> {
    api_post("/api/v1/projects/query", req).await
}

pub async fn search_projects(
    req: &SearchProjectsRequest,
) -> Result<PagedResult<ProjectListItem>, ApiError> {
    api_post("/api/v1/projects/search", req).await
}

pub async fn get_project(req: GetProjectRequest) -> Result<GetProjectResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("with_stats", req.with_stats.map(|v| v.to_string())),
        (
            "with_model_call_stats",
            req.with_model_call_stats.map(|v| v.to_string()),
        ),
        (
            "with_task_graph",
            req.with_task_graph.map(|v| v.to_string()),
        ),
        (
            "stats_time_start",
            req.stats_time_start.map(|v| v.to_string()),
        ),
        ("stats_time_end", req.stats_time_end.map(|v| v.to_string())),
        ("stats_interval", req.stats_interval.clone()),
        ("with_artifacts", req.with_artifacts.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/projects/{}{}", req.id, qs)).await
}

pub async fn create_project(req: CreateProjectRequest) -> Result<CreateProjectResponse, ApiError> {
    api_post("/api/v1/projects", &req).await
}

pub async fn update_project(req: UpdateProjectRequest) -> Result<UpdateProjectResponse, ApiError> {
    api_put(&format!("/api/v1/projects/{}", req.id), &req).await
}

pub async fn update_project_status(req: UpdateProjectStatusRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": req.status as i32 });
    api_put_empty(&format!("/api/v1/projects/{}/status", req.id), &body).await
}

// ===== 任务管理 =====

pub async fn list_project_tasks(project_id: &str) -> Result<ListTasksResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/projects/{}/tasks", project_id)).await
}

#[allow(dead_code)]
pub async fn list_tasks(req: ListTasksRequest) -> Result<PagedResult<TaskListItem>, ApiError> {
    let url = super::build_pagination_url("/api/v1/tasks", &req.pagination);
    api_get(&url).await
}

pub async fn query_tasks(req: &TaskQueryRequest) -> Result<PagedResult<TaskListItem>, ApiError> {
    api_post("/api/v1/tasks/query", req).await
}

pub async fn search_tasks(req: &SearchTasksRequest) -> Result<PagedResult<TaskListItem>, ApiError> {
    api_post("/api/v1/tasks/search", req).await
}

pub async fn get_task(req: GetTaskRequest) -> Result<GetTaskResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("with_stats", req.with_stats.map(|v| v.to_string())),
        (
            "with_model_call_stats",
            req.with_model_call_stats.map(|v| v.to_string()),
        ),
        (
            "stats_time_start",
            req.stats_time_start.map(|v| v.to_string()),
        ),
        ("stats_time_end", req.stats_time_end.map(|v| v.to_string())),
        ("stats_interval", req.stats_interval.clone()),
        ("with_artifacts", req.with_artifacts.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/tasks/{}{}", req.id, qs)).await
}

pub async fn create_task(req: CreateTaskRequest) -> Result<CreateTaskResponse, ApiError> {
    api_post("/api/v1/tasks", &req).await
}

pub async fn update_task(req: UpdateTaskRequest) -> Result<UpdateTaskResponse, ApiError> {
    api_put(&format!("/api/v1/tasks/{}", req.id), &req).await
}

pub async fn update_task_status(req: UpdateTaskStatusRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": req.status as i32 });
    api_put_empty(&format!("/api/v1/tasks/{}/status", req.id), &body).await
}

pub async fn update_task_progress(
    req: UpdateTaskProgressRequest,
) -> Result<GetTaskResponse, ApiError> {
    api_put(&format!("/api/v1/tasks/{}/progress", req.id), &req).await
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

pub async fn update_artifact(
    req: common::api::UpdateArtifactRequest,
) -> Result<ArtifactDetail, ApiError> {
    api_put(
        &format!("/api/v1/project/artifacts/{}", req.artifact_id),
        &req,
    )
    .await
}

#[allow(dead_code)]
pub async fn get_artifact(id: &str) -> Result<ArtifactDetail, ApiError> {
    api_get(&format!("/api/v1/project/artifacts/{}", id)).await
}
