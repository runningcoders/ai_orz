//! Handler: GET /api/v1/tasks - List tasks globally with optional filtering

use super::response;
use crate::pkg::RequestContext;
use crate::service::dao::task::TaskQuery;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListTasksRequest, PagedResult, TaskListItem};
use common::error::Result;

/// List tasks globally with optional filtering by project, status, assignee, etc.
#[register_handler_tool(
    id = "list_tasks",
    name = "List All Tasks",
    description = "List tasks globally with pagination. Lightweight list scenario (GET + query params), excludes deleted tasks.",
    params = "common::api::ListTasksRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn list_tasks(
    ctx: RequestContext,
    params: ListTasksRequest,
) -> Result<PagedResult<TaskListItem>> {
    // list 是语法糖：只接受分页，内部固定排除 status=0
    let page = domain()
        .task_manage()
        .query(
            ctx,
            TaskQuery {
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(|t| response::to_list_item(&t)))
}
