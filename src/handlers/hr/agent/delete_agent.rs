//! Handler: DELETE /api/v1/agents/{id} - Delete an agent

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{DeleteAgentRequest, DeleteAgentResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

/// Delete an existing AI agent
#[register_handler_tool(
    id = "delete_agent",
    name = "delete_agent",
    description = "Delete an existing AI agent",
    params = "common::api::DeleteAgentRequest",
)]
#[generate_http_handler]
pub async fn delete_agent(
    ctx: RequestContext,
    params: DeleteAgentRequest,
) -> Result<DeleteAgentResponse, AppError> {
    let agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", params.id)))?;

    domain().agent_manage().delete_agent(ctx, &agent).await?;

    Ok(DeleteAgentResponse { success: true })
}
