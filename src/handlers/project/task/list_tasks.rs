//! Handler: GET /api/v1/tasks - List tasks globally with optional filtering

use super::response;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::task::TaskQuery;
use crate::service::domain::project::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{ListTasksRequest, PagedResult, TaskListItem};

/// List tasks globally with optional filtering by project, status, assignee, etc.
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
