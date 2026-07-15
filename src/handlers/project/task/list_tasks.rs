//! Handler: GET /api/v1/tasks - List tasks globally with optional filtering

use super::response;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{ListTasksRequest, TaskListItem};
use common::enums::{AssigneeType, TaskStatus};

/// List tasks globally with optional filtering by project, status, assignee, etc.
#[generate_http_handler]
pub async fn list_tasks(
    ctx: RequestContext,
    params: ListTasksRequest,
) -> Result<Vec<TaskListItem>> {
    let status = params.status.map(TaskStatus::from_i32);
    let assignee_type = params.assignee_type.map(AssigneeType::from_i32);
    let assignee_id = params.assignee_id.as_deref();
    let project_id = params.project_id.as_deref();

    let tasks = domain()
        .task_manage()
        .list(
            ctx,
            project_id,
            assignee_type,
            assignee_id,
            status,
            params.limit,
        )
        .await?;
    let response_items = tasks.iter().map(response::to_list_item).collect();

    Ok(response_items)
}
