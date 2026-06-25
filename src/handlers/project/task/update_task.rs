//! Handler: PUT /api/v1/tasks/{id} - Update task basic information

use super::response;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateTaskRequest, UpdateTaskResponse};
use common::bail_err;

/// Update task basic information (title, description, priority, tags, etc.)
#[register_handler_tool(
    id = "update_task",
    name = "update_task",
    description = "Update basic information of an existing task",
    params = "common::api::UpdateTaskRequest"
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
        )
        .await?;

    Ok(response::to_detail(&task))
}
