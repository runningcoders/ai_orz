//! 列出 Agent 已安装 Skill

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, SkillListItem};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

use super::response::to_list_item;

/// 列出 Agent 已安装 Skill
/// GET /hr/agents/{agent_id}/skills
pub async fn list_agent_skills(
    Extension(ctx): Extension<RequestContext>,
    Path(agent_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<SkillListItem>>>, AppError> {
    let skills = domain()
        .skill_manage()
        .list_for_agent(ctx, &agent_id)
        .await?;

    let responses = skills.iter().map(to_list_item).collect();
    Ok(Json(ApiResponse::success(responses)))
}
