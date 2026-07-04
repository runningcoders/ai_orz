//! Handler: DELETE /api/v1/agents/{id} - Delete an agent

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteAgentRequest, DeleteAgentResponse};

use crate::enrich_ctx;

/// Delete an existing AI agent
#[register_handler_tool(
    id = "delete_agent",
    name = "delete_agent",
    description = "Delete an existing AI agent",
    params = "common::api::DeleteAgentRequest"
)]
#[generate_http_handler]
pub async fn delete_agent(
    ctx: RequestContext,
    params: DeleteAgentRequest,
) -> Result<DeleteAgentResponse> {
    let agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &agent);

    domain().agent_manage().delete_agent(ctx, &agent).await?;

    Ok(DeleteAgentResponse { success: true })
}
