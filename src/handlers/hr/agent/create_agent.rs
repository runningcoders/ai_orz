//! 创建 Agent

use common::api::{CreateAgentRequest, CreateAgentResponse};
use common::constants::RequestContext;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::models::agent::{Agent, AgentPo};
use crate::service::domain::hr::domain;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
};

/// 创建 Agent
/// POST /agents
pub async fn create_agent(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateAgentResponse>>), AppError> {

    let agent_po = AgentPo::new(
        req.name.clone(),
        req.role.clone(),
        req.description.unwrap_or_default(),
        req.capabilities.unwrap_or_default(),
        req.soul.unwrap_or_default(),
        req.model_provider_id.clone(),
        ctx.uid().to_string(),
    );
    let agent = Agent::from_po(agent_po);

    domain().agent_manage().create_agent(ctx, &agent)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(CreateAgentResponse {
            id: agent.id().to_string(),
            name: agent.name().to_string(),
            description: if agent.po.description.is_empty() { None } else { Some(agent.po.description.clone()) },
            created_at: agent.po.created_at,
        })),
    ))
}
