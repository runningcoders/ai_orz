//! 列出所有 Agent

use common::api::{AgentListItem};
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use axum::{
    extract::{Extension},
    Json,
};

/// 列出所有 Agent
/// GET /agents
pub async fn list_agents(
    Extension(ctx): Extension<RequestContext>
) -> Result<Json<ApiResponse<Vec<AgentListItem>>>, AppError> {

    let agents = domain().agent_manage().list_agents(ctx).await?;
    let responses: Vec<AgentListItem> = agents
        .iter()
        .map(|agent| AgentListItem {
            id: agent.id().to_string(),
            name: agent.name().to_string(),
            role: if agent.po.role.as_ref().map_or(true, |r| r.is_empty()) { None } else { agent.po.role.clone() },
            description: if agent.po.description.as_ref().map_or(true, |d| d.is_empty()) { None } else { agent.po.description.clone() },
            model_provider_id: agent.po.model_provider_id.clone().expect("model_provider_id should not be None"),
            created_at: agent.po.created_at,
        })
        .collect();

    Ok(Json(ApiResponse::success(responses)))
}
