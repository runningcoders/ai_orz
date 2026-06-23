//! Handler: PUT /api/v1/tasks/{id}/status - Update task status (state transition)

use super::response;
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateTaskStatusRequest, UpdateTaskStatusResponse};

/// Update task status with state transition validation
#[register_handler_tool(
    id = "update_task_status",
    name = "update_task_status",
    description = "Update the status of a task with proper state transition validation",
    params = "common::api::UpdateTaskStatusRequest"
)]
#[generate_http_handler]
pub async fn update_task_status(
    ctx: RequestContext,
    params: UpdateTaskStatusRequest,
) -> Result<UpdateTaskStatusResponse, AppError> {
    let mut task = domain()
        .task_manage()
        .get(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {} not found", params.id)))?;

    domain()
        .task_manage()
        .transition_status(ctx, &mut task, params.status)
        .await?;

    Ok(response::to_detail(&task))
}
