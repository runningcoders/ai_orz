//! Handler: POST /api/v1/tasks/query - Task 通用查询接口
//!
//! 与 list_tasks 的区别：list 是列表场景语法糖（GET + query param），
//! query 是完整查询能力（POST + body），支持复杂组合过滤。

use super::response;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::task::TaskQuery;
use crate::service::domain::project::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{TaskListItem, TaskQueryRequest};

/// Task 通用查询（POST body，支持完整查询能力）
#[generate_http_handler]
pub async fn query_tasks(
    ctx: RequestContext,
    params: TaskQueryRequest,
) -> Result<Vec<TaskListItem>> {
    let tasks = domain()
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
                limit: params.limit,
                ..Default::default()
            },
        )
        .await?;
    let response_items = tasks.iter().map(response::to_list_item).collect();

    Ok(response_items)
}
