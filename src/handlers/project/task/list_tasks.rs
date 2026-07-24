//! Handler: GET /api/v1/tasks - List tasks globally with optional filtering

use super::response;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::task::TaskQuery;
use crate::service::domain::project::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{ListTasksRequest, ListTasksResponse, TaskListItem};
use common::enums::{AssigneeType, TaskStatus};

/// List tasks globally with optional filtering by project, status, assignee, etc.
#[generate_http_handler]
pub async fn list_tasks(
    ctx: RequestContext,
    params: ListTasksRequest,
) -> Result<ListTasksResponse> {
    // 走通用 query（list 是语法糖，handler 内部统一用 query）
    let assignee_type = params.assignee_type.map(AssigneeType::from_i32);
    let status_in = params.status.map(|s| vec![TaskStatus::from_i32(s)]);
    let tasks = domain()
        .task_manage()
        .query(
            ctx,
            TaskQuery {
                project_id: params.project_id,
                assignee_type,
                assignee_id: params.assignee_id,
                status_in,
                ids: params.ids,
                limit: params.limit,
                ..Default::default()
            },
        )
        .await?;
    let tasks: Vec<TaskListItem> = tasks.iter().map(response::to_list_item).collect();

    Ok(ListTasksResponse { tasks })
}
