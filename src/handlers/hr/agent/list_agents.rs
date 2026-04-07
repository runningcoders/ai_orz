//! 列出所有 Agent

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use crate::service::domain::hr::domain;
use axum::{
    http::HeaderMap,
    Json,
};
use serde::{Serialize};

/// Agent 列表项响应
#[derive(Debug, Serialize)]
pub struct AgentListItem {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub model_provider_id: String,
    pub created_at: i64,
}

/// 列出所有 Agent
/// GET /agents
pub async fn list_agents(headers: HeaderMap) -> Result<Json<ApiResponse<Vec<AgentListItem>>>, AppError> {
    let ctx = extract_ctx(&headers);

    let agents = domain().agent_manage().list_agents(ctx)?;
    let responses: Vec<AgentListItem> = agents
        .iter()
        .map(|agent| AgentListItem {
            id: agent.id().to_string(),
            name: agent.name().to_string(),
            description: if agent.po.description.is_empty() { None } else { Some(agent.po.description.clone()) },
            model_provider_id: agent.po.model_provider_id.clone(),
            created_at: agent.po.created_at,
        })
        .collect();

    Ok(Json(ApiResponse::success(responses)))
}
