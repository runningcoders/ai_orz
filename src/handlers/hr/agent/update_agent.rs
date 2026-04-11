//! 更新 Agent

use common::api::{UpdateAgentRequest, UpdateAgentResponse};
use crate::pkg::RequestContext;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::service::domain::hr::domain;
use axum::{
    extract::{Extension, Path, Json},
};
use std::time::{SystemTime, UNIX_EPOCH};

/// 更新时间戳
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// 更新 Agent
/// PUT /agents/{id}
pub async fn update_agent(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<ApiResponse<UpdateAgentResponse>>, AppError> {

    let mut agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    // 更新字段
    if let Some(name) = req.name {
        agent.po.name = Some(name);
    }
    if let Some(description) = req.description {
        agent.po.description = Some(description);
    }
    if let Some(capabilities) = req.capabilities {
        agent.po.capabilities = Some(serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string()));
    }
    if let Some(soul) = req.soul {
        agent.po.soul = Some(soul);
    }
    if let Some(model_provider_id) = req.model_provider_id {
        agent.po.model_provider_id = Some(model_provider_id);
    }
    // 更新 modified_by 和 updated_at
    agent.po.modified_by = Some(ctx.uid());
    agent.po.updated_at = current_timestamp();

    domain().agent_manage().update_agent(ctx, &agent).await?;

    let capabilities: Vec<String> = agent.po.get_capabilities();

    Ok(Json(ApiResponse::success(UpdateAgentResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        description: if agent.po.description.as_ref().map_or(true, |d| d.is_empty()) { None } else { agent.po.description.clone() },
        capabilities: if capabilities.is_empty() { None } else { Some(capabilities) },
        soul: if agent.po.soul.as_ref().map_or(true, |s| s.is_empty()) { None } else { agent.po.soul.clone() },
        model_provider_id: agent.po.model_provider_id.clone().expect("model_provider_id should not be None"),
        updated_at: agent.po.updated_at,
    })))
}
