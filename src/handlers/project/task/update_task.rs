//! Handler: PUT /api/v1/tasks/{id} - Update task basic information

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateTaskRequest, UpdateTaskResponse};
use common::error::Result;

/// Update task basic information (title, description, priority, tags, etc.)
#[register_handler_tool(
    id = "update_task",
    name = "Update Task",
    description = "Partially update a task's basic info: title, description, priority, tags, due date, dependency task IDs, execution plan, and execution result; only provided fields change. Returns the updated task detail. This does not change status or progress — use update_task_status or update_task_progress for those.",
    params = "common::api::UpdateTaskRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_task(
    ctx: RequestContext,
    params: UpdateTaskRequest,
) -> Result<UpdateTaskResponse> {
    let task = domain()
        .task_manage()
        .update_basic(
            ctx,
            &params.id,
            params.title,
            params.description,
            params.priority,
            params.tags,
            params.due_at,
            params.dependencies,
            params.execution_plan,
            params.execution_result,
        )
        .await?;

    Ok(response::to_detail(&task))
}
