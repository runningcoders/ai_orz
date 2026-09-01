//! Handler: POST /api/v1/tasks/search - Search tasks with full filtering
//!
//! 与 query_tasks 的区别：search 重在"语义相关性"（FTS5 + 向量语义混合搜索），
//! query 重在"条件过滤"。两者现在都支持完整过滤条件和分页返回。

use super::response;
use crate::pkg::RequestContext;
use crate::service::dao::task::{TaskQuery, TaskSearch};
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, SearchTasksRequest, TaskListItem};
use common::error::Result;

/// Search tasks with full filtering (FTS5 + vector semantic search)
#[register_handler_tool(
    id = "search_tasks",
    name = "Search Tasks",
    description = "Search tasks by keyword with full filtering support (FTS5 + vector semantic search).",
    params = "common::api::SearchTasksRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn search_tasks(
    ctx: RequestContext,
    params: SearchTasksRequest,
) -> Result<PagedResult<TaskListItem>> {
    let search = TaskSearch {
        keyword: params.keyword,
        filters: TaskQuery {
            ids: params.ids,
            project_id: params.project_id,
            assignee_type: params.assignee_type,
            assignee_id: params.assignee_id,
            status_in: params.status_in,
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().task_manage().search(ctx, search).await?;

    Ok(page.map(|t| response::to_list_item(&t)))
}
