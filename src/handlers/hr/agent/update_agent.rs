//! Handler: PUT /api/v1/agents/{id} - Update agent information

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateAgentRequest, UpdateAgentResponse};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::enrich_ctx;

/// Update the metadata and configuration of an existing AI agent
#[register_handler_tool(
    id = "update_agent",
    name = "update_agent",
    description = "Update the metadata and configuration of an existing AI agent",
    params = "common::api::UpdateAgentRequest"
)]
#[generate_http_handler]
pub async fn update_agent(
    ctx: RequestContext,
    params: UpdateAgentRequest,
) -> Result<UpdateAgentResponse> {
    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    let mut agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &agent);

    // Update fields
    if let Some(name) = params.name {
        agent.po.name = name;
    }
    if let Some(description) = params.description {
        agent.po.description = description;
    }
    if let Some(capabilities) = params.capabilities {
        agent.po.capabilities =
            serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(soul) = params.soul {
        agent.po.soul = soul;
    }
    if let Some(model_provider_id) = params.model_provider_id {
        agent.po.model_provider_id = model_provider_id;
    }
    // Update modified_by and updated_at
    agent.po.modified_by = ctx.uid();
    agent.po.updated_at = current_timestamp();

    domain().agent_manage().update_agent(ctx, &agent).await?;

    let capabilities: Vec<String> = agent.po.get_capabilities();

    Ok(UpdateAgentResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        description: if agent.po.description.is_empty() {
            None
        } else {
            Some(agent.po.description.clone())
        },
        capabilities: if capabilities.is_empty() {
            None
        } else {
            Some(capabilities)
        },
        soul: if agent.po.soul.is_empty() {
            None
        } else {
            Some(agent.po.soul.clone())
        },
        model_provider_id: agent.po.model_provider_id.clone(),
        updated_at: agent.po.updated_at,
    })
}
