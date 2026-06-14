//! 绑定 Tool 到 Agent

use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, BindToolToAgentRequest, BindToolToAgentResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// 绑定 Tool 到 Agent
/// POST /tools/{id}/agent-bind
pub async fn bind_tool_to_agent(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<BindToolToAgentRequest>,
) -> Result<AxumJson<ApiResponse<BindToolToAgentResponse>>, AppError> {
    domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", id)))?;

    domain()
        .tool_provider_manage()
        .bind_tool_to_agent(ctx, &req.agent_id, &id)
        .await?;

    Ok(AxumJson(ApiResponse::success(BindToolToAgentResponse {
        agent_id: req.agent_id,
        tool_id: id,
    })))
}
