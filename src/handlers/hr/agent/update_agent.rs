//! 更新 Agent

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use crate::service::domain::hr::domain;
use axum::{
    extract::{Path, Json},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// 更新 Agent 请求
#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    /// Agent 名称
    pub name: Option<String>,
    /// Agent 描述
    pub description: Option<String>,
    /// 能力列表 JSON
    pub capabilities: Option<Vec<String>>,
    /// Agent 灵魂提示词
    pub soul: Option<String>,
}

/// 更新 Agent 响应
#[derive(Debug, Serialize)]
pub struct UpdateAgentResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub soul: Option<String>,
    pub model_provider_id: String,
    pub updated_at: i64,
}

/// 更新时间戳
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// 更新 Agent
/// PUT /agents/:id
pub async fn update_agent(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<ApiResponse<UpdateAgentResponse>>, AppError> {
    let ctx = extract_ctx(&headers);

    let mut agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    // 更新字段
    if let Some(name) = req.name {
        agent.po.name = name;
    }
    if let Some(description) = req.description {
        agent.po.description = description;
    }
    if let Some(capabilities) = req.capabilities {
        agent.po.capabilities = serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(soul) = req.soul {
        agent.po.soul = soul;
    }
    // 更新 modified_by 和 updated_at
    agent.po.modified_by = ctx.uid();
    agent.po.updated_at = current_timestamp();

    domain().agent_manage().update_agent(ctx, &agent)?;

    let capabilities: Vec<String> = agent.po.get_capabilities();

    Ok(Json(ApiResponse::success(UpdateAgentResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        description: if agent.po.description.is_empty() { None } else { Some(agent.po.description.clone()) },
        capabilities: if capabilities.is_empty() { None } else { Some(capabilities) },
        soul: if agent.po.soul.is_empty() { None } else { Some(agent.po.soul.clone()) },
        model_provider_id: agent.po.model_provider_id.clone(),
        updated_at: agent.po.updated_at,
    })))
}
