//! Handler: PUT /api/v1/tools/{id}/status - Update tool status (enable/disable)

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateToolStatusRequest, UpdateToolStatusResponse};

use super::response::to_detail;

/// Update the status of a tool (enable/disable it)
#[register_handler_tool(
    id = "update_tool_status",
    name = "update_tool_status",
    description = "Update the status of a tool (enable/disable it)",
    params = "common::api::UpdateToolStatusRequest"
)]
#[generate_http_handler]
pub async fn update_tool_status(
    ctx: RequestContext,
    params: UpdateToolStatusRequest,
) -> Result<UpdateToolStatusResponse, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let mut tool = domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", params.id)))?;

    tool.transition_status(params.status, user_id)
        .map_err(AppError::BadRequest)?;

    domain()
        .tool_provider_manage()
        .update_tool(ctx, &tool)
        .await?;

    Ok(to_detail(&tool))
}
