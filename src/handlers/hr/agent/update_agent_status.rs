//! 更新 Agent 状态

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, UpdateAgentStatusRequest, UpdateAgentStatusResponse};

/// 更新 Agent 状态
/// PUT /agents/{id}/status
pub async fn update_agent_status(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentStatusRequest>,
) -> Result<Json<ApiResponse<UpdateAgentStatusResponse>>, AppError> {
    let mut agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    domain()
        .agent_manage()
        .transition_status(ctx, &mut agent, req.status)
        .await?;

    Ok(Json(ApiResponse::success(UpdateAgentStatusResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        roles: agent.po.get_roles(),
        description: if agent.po.description.is_empty() {
            None
        } else {
            Some(agent.po.description.clone())
        },
        capabilities: {
            let capabilities = agent.po.get_capabilities();
            if capabilities.is_empty() {
                None
            } else {
                Some(capabilities)
            }
        },
        soul: if agent.po.soul.is_empty() {
            None
        } else {
            Some(agent.po.soul.clone())
        },
        model_provider_id: agent.po.model_provider_id.clone(),
        status: agent.po.status as i32,
        created_at: agent.po.created_at,
        updated_at: agent.po.updated_at,
    })))
}
