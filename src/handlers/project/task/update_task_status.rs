//! Handler: PUT /api/v1/tasks/{id}/status - Update task status (state transition)

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateTaskStatusRequest, UpdateTaskStatusResponse};
use common::error::Result;

use crate::enrich_ctx;

/// Update task status with state transition validation
#[register_handler_tool(
    id = "update_task_status",
    name = "Update Task Status",
    description = "Update the status of a task with proper state transition validation",
    params = "common::api::UpdateTaskStatusRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_task_status(
    ctx: RequestContext,
    params: UpdateTaskStatusRequest,
) -> Result<UpdateTaskStatusResponse> {
    let task = domain()
        .task_manage()
        .get(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Task {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &task);
    let mut task = task;

    domain()
        .task_manage()
        .transition_status(ctx, &mut task, params.status)
        .await?;

    Ok(response::to_detail(&task))
}
