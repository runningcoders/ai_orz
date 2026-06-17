//! 列出 Agent 被分配的 Task

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    Json,
    extract::{Extension, Path, Query},
};
use common::api::{ApiResponse, ListTasksQuery, TaskListItem};
use common::enums::AssigneeType;

use super::response;

/// 列出 Agent 被分配的 Task
/// GET /agents/{agent_id}/tasks
pub async fn list_agent_tasks(
    Extension(ctx): Extension<RequestContext>,
    Path(agent_id): Path<String>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<ApiResponse<Vec<TaskListItem>>>, AppError> {
    let tasks = domain()
        .task_manage()
        .list(
            ctx,
            None,
            Some(AssigneeType::Agent),
            Some(&agent_id),
            query.status,
            query.limit,
        )
        .await?;
    let response_items = tasks.iter().map(response::to_list_item).collect();

    Ok(Json(ApiResponse::success(response_items)))
}
