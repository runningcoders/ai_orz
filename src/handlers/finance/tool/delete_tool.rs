//! Handler: DELETE /api/v1/tools/{id} - Delete a custom tool

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteToolRequest, DeleteToolResponse};

/// Delete an existing custom tool (soft delete)
#[register_handler_tool(
    id = "delete_tool",
    name = "delete_tool",
    description = "Delete an existing custom tool (soft delete)",
    params = "common::api::DeleteToolRequest"
)]
#[generate_http_handler]
pub async fn delete_tool(
    ctx: RequestContext,
    params: DeleteToolRequest,
) -> Result<DeleteToolResponse> {
    let tool = domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Tool {} not found", params.id)))?;

    domain()
        .tool_provider_manage()
        .delete_tool(ctx, &tool)
        .await?;

    Ok(DeleteToolResponse { success: true })
}
