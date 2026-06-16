//! 安装 Skill 到 Agent

use axum::{
    Json,
    extract::{Extension, Path},
    http::StatusCode,
};
use common::api::{ApiResponse, InstallSkillToAgentResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

use super::response::to_detail;

/// 安装 Skill 到 Agent
/// POST /hr/agents/{agent_id}/skills/{skill_id}
pub async fn install_skill_to_agent(
    Extension(ctx): Extension<RequestContext>,
    Path((agent_id, skill_id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<ApiResponse<InstallSkillToAgentResponse>>), AppError> {
    let skill = domain()
        .skill_manage()
        .install_to_agent(ctx, &skill_id, &agent_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(InstallSkillToAgentResponse {
            agent_id,
            source_skill_id: skill_id,
            skill: to_detail(&skill),
        })),
    ))
}
