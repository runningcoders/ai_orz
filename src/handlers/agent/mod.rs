//! Agent Handler

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::RequestContext;
use crate::service::domain::agent::domain;
use axum::{
    extract::{Json, Path},
    http::{self, HeaderMap},
    http::StatusCode,
};

pub mod dto;
pub use dto::{AgentResponse, CreateAgentRequest, UpdateAgentRequest};

/// 从 HeaderMap 提取 RequestContext
fn extract_ctx(headers: &HeaderMap) -> RequestContext {
    let user_id = headers
        .get("X-User-Id")
        .and_then(|v: &http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.to_string());
    let user_name = headers
        .get("X-User-Name")
        .and_then(|v: &http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.to_string());
    RequestContext::new(user_id, user_name)
}

/// 创建 Agent
///
/// POST /agents
pub async fn create_agent(
    headers: HeaderMap,
    Json(req): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<AgentResponse>>), AppError> {
    let ctx = extract_ctx(&headers);

    let agent_po = AgentPo::new(
        req.name,
        req.role,
        req.capabilities,
        req.soul,
        ctx.uid().to_string(),
    );
    let agent = Agent::from_po(agent_po);

    domain().create(ctx, &agent)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(AgentResponse::from_agent(&agent))),
    ))
}

/// 获取 Agent
///
/// GET /agents/:id
pub async fn get_agent(
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<AgentResponse>>, AppError> {
    let ctx = extract_ctx(&headers);

    let agent = domain()
        .get(ctx, &id)?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    Ok(Json(ApiResponse::success(AgentResponse::from_agent(
        &agent,
    ))))
}

/// 列出所有 Agent
///
/// GET /agents
pub async fn list_agents(headers: HeaderMap) -> Result<Json<ApiResponse<Vec<AgentResponse>>>, AppError> {
    let ctx = extract_ctx(&headers);

    let agents = domain().list(ctx)?;
    let responses: Vec<AgentResponse> = agents.iter().map(AgentResponse::from_agent).collect();

    Ok(Json(ApiResponse::success(responses)))
}

/// 更新 Agent
///
/// PUT /agents/:id
pub async fn update_agent(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<ApiResponse<AgentResponse>>, AppError> {
    let ctx = extract_ctx(&headers);

    let mut agent = domain()
        .get(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    // 更新字段
    if let Some(name) = req.name {
        agent.po.name = name;
    }
    if let Some(role) = req.role {
        agent.po.role = role;
    }
    if let Some(capabilities) = req.capabilities {
        agent.po.capabilities = serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(soul) = req.soul {
        agent.po.soul = soul;
    }
    // 更新 modified_by 为当前用户
    agent.po.modified_by = ctx.uid();

    domain().update(ctx, &agent)?;

    Ok(Json(ApiResponse::success(AgentResponse::from_agent(&agent))))
}

/// 删除 Agent
///
/// DELETE /agents/:id
pub async fn delete_agent(
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let ctx = extract_ctx(&headers);

    let agent = domain()
        .get(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    domain().delete(ctx, &agent)?;

    Ok(Json(ApiResponse::<()>::ok()))
}
