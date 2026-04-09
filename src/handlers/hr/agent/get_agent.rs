//! 获取单个 Agent

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use axum::{
    extract::{Extension, Path},
    Json,
};
use serde::{Serialize};

/// 获取 Agent 响应
#[derive(Debug, Serialize)]
pub struct GetAgentResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub soul: Option<String>,
    pub model_provider_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 获取 Agent
/// GET /agents/:id
pub async fn get_agent(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetAgentResponse>>, AppError> {

    let agent = domain()
        .agent_manage()
        .get_agent(ctx, &id)?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    let capabilities: Vec<String> = agent.po.get_capabilities();

    Ok(Json(ApiResponse::success(GetAgentResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        description: if agent.po.description.is_empty() { None } else { Some(agent.po.description.clone()) },
        capabilities: if capabilities.is_empty() { None } else { Some(capabilities) },
        soul: if agent.po.soul.is_empty() { None } else { Some(agent.po.soul.clone()) },
        model_provider_id: agent.po.model_provider_id.clone(),
        created_at: agent.po.created_at,
        updated_at: agent.po.updated_at,
    })))
}
