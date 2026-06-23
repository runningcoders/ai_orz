//! Handler: GET /api/v1/tools/{id} - Get tool detailed information

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetToolRequest, GetToolResponse};

use super::response::to_detail;

/// Get detailed information about a specific tool including configuration
#[register_handler_tool(
    id = "get_tool",
    name = "get_tool",
    description = "Get detailed information about a specific tool including configuration",
    params = "common::api::GetToolRequest"
)]
#[generate_http_handler]
pub async fn get_tool(
    ctx: RequestContext,
    params: GetToolRequest,
) -> Result<GetToolResponse, AppError> {
    let tool = domain()
        .tool_provider_manage()
        .get_tool(ctx, &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", params.id)))?;

    Ok(to_detail(&tool))
}
