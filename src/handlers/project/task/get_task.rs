//! Handler: GET /api/v1/tasks/{id} - Get task detailed information

use super::response;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetTaskRequest, GetTaskResponse};
use common::bail_err;

/// Get task detailed information by ID
#[register_handler_tool(
    id = "get_task",
    name = "get_task",
    description = "Get detailed information about a specific task by its ID",
    params = "common::api::GetTaskRequest"
)]
#[generate_http_handler]
pub async fn get_task(
    ctx: RequestContext,
    params: GetTaskRequest,
) -> Result<GetTaskResponse> {
    let task = domain()
        .task_manage()
        .get(ctx, &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Task {} not found", params.id)))?;

    Ok(response::to_detail(&task))
}
