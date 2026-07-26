//! Handler: GET /api/v1/projects/{project_id}/tasks - List tasks under a project

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListProjectTasksRequest, TaskListItem};
use common::error::Result;

/// List all tasks under a specific project, with optional status filtering
#[register_handler_tool(
    id = "list_project_tasks",
    name = "list_project_tasks",
    description = "List all tasks under a specific project, with optional status filtering",
    params = "common::api::ListProjectTasksRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn list_project_tasks(
    ctx: RequestContext,
    params: ListProjectTasksRequest,
) -> Result<Vec<TaskListItem>> {
    let tasks = domain()
        .task_manage()
        .list(
            ctx,
            Some(&params.project_id),
            None,
            None,
            params.status,
            params.limit,
        )
        .await?;
    let response_items = tasks.iter().map(response::to_list_item).collect();

    Ok(response_items)
}
