//! 列出 Project 下的 Task

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    Json,
    extract::{Extension, Path, Query},
};
use common::api::{ApiResponse, ListTasksQuery, TaskListItem};

use super::response;

/// 列出 Project 下的 Task
/// GET /projects/{project_id}/tasks
pub async fn list_project_tasks(
    Extension(ctx): Extension<RequestContext>,
    Path(project_id): Path<String>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<ApiResponse<Vec<TaskListItem>>>, AppError> {
    let tasks = domain()
        .task_manage()
        .list(
            ctx,
            Some(&project_id),
            None,
            None,
            query.status,
            query.limit,
        )
        .await?;
    let response_items = tasks.iter().map(response::to_list_item).collect();

    Ok(Json(ApiResponse::success(response_items)))
}
