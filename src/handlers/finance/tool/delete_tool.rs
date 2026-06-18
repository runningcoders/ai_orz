//! Handler: DELETE /api/v1/tools/{id} - Delete a custom tool

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{DeleteToolRequest, DeleteToolResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// Delete an existing custom tool (soft delete)
#[register_handler_tool(
    id = "delete_tool",
    name = "delete_tool",
    description = "Delete an existing custom tool (soft delete)",
    params = "common::api::DeleteToolRequest",
)]
#[generate_http_handler]
pub async fn delete_tool(
    ctx: RequestContext,
    params: DeleteToolRequest,
) -> Result<DeleteToolResponse, AppError> {
    let tool = domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", params.id)))?;

    domain()
        .tool_provider_manage()
        .delete_tool(ctx, &tool)
        .await?;

    Ok(DeleteToolResponse { success: true })
}