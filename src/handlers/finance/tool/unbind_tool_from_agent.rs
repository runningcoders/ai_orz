//! 解绑 Tool 与 Agent

use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, UnbindToolFromAgentRequest, UnbindToolFromAgentResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// 解绑 Tool 与 Agent
/// DELETE /tools/{id}/agent-bind
pub async fn unbind_tool_from_agent(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UnbindToolFromAgentRequest>,
) -> Result<AxumJson<ApiResponse<UnbindToolFromAgentResponse>>, AppError> {
    domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", id)))?;

    domain()
        .tool_provider_manage()
        .unbind_tool_from_agent(ctx, &req.agent_id, &id)
        .await?;

    Ok(AxumJson(ApiResponse::success(
        UnbindToolFromAgentResponse {
            agent_id: req.agent_id,
            tool_id: id,
        },
    )))
}
