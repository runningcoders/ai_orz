//! Handler: POST /api/v1/tasks/query - Task 通用查询接口
//!
//! 与 list_tasks 的区别：list 是列表场景语法糖（GET + query param），
//! query 是完整查询能力（POST + body），支持复杂组合过滤。

use super::response;
use crate::pkg::RequestContext;
use crate::service::dao::task::TaskQuery;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, TaskListItem, TaskQueryRequest};
use common::error::Result;

/// Task 通用查询（POST body，支持完整查询能力）
#[register_handler_tool(
    id = "query_tasks",
    name = "query_tasks",
    description = "Query tasks with full filtering support (ids, keyword, project_id, assignee, status_in, pagination). POST body for complex combinations.",
    params = "common::api::TaskQueryRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn query_tasks(
    ctx: RequestContext,
    params: TaskQueryRequest,
) -> Result<PagedResult<TaskListItem>> {
    let page = domain()
        .task_manage()
        .query(
            ctx,
            TaskQuery {
                ids: params.ids,
                keyword: params.keyword,
                project_id: params.project_id,
                assignee_type: params.assignee_type,
                assignee_id: params.assignee_id,
                status_in: params.status_in,
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(|t| response::to_list_item(&t)))
}
