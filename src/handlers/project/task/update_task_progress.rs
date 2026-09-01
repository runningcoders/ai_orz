//! Handler: PUT /api/v1/tasks/{id}/progress - 更新任务进度

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateTaskProgressRequest, UpdateTaskProgressResponse};
use common::error::Result;

/// 更新任务进度（0-100）
#[register_handler_tool(
    id = "update_task_progress",
    name = "Update Task Progress",
    description = "Update task progress (0-100)",
    params = "common::api::UpdateTaskProgressRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_task_progress(
    ctx: RequestContext,
    params: UpdateTaskProgressRequest,
) -> Result<UpdateTaskProgressResponse> {
    let task = domain()
        .task_manage()
        .update_progress(ctx, &params.id, params.progress)
        .await?;

    Ok(response::to_detail(&task))
}
