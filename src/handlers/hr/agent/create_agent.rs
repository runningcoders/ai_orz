//! Handler: POST /api/v1/agents - 创建新 Agent

use crate::error::AppError;
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateAgentRequest, CreateAgentResponse};

/// Create a new AI agent
#[register_handler_tool(
    id = "create_agent",
    name = "create_agent",
    description = "Create a new AI agent with specified configuration",
    params = "common::api::CreateAgentRequest"
)]
#[generate_http_handler]
pub async fn create_agent(
    ctx: RequestContext,
    params: CreateAgentRequest,
) -> Result<CreateAgentResponse, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let agent_po = AgentPo::new(
        params.name.clone(),
        params.roles.unwrap_or_default(),
        params.description.unwrap_or_default(),
        params.capabilities.unwrap_or_default(),
        params.soul.unwrap_or_default(),
        params.model_provider_id.clone(),
        user_id.to_string(),
    );
    let agent = Agent::from_po(agent_po);

    domain()
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await?;

    let created = domain()
        .agent_manage()
        .get_agent(ctx, agent.id())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent.id())))?;

    Ok(CreateAgentResponse {
        id: created.id().to_string(),
        name: created.name().to_string(),
        description: if created.po.description.is_empty() {
            None
        } else {
            Some(created.po.description.clone())
        },
        created_at: created.po.created_at,
    })
}
