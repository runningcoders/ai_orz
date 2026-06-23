//! Handler: GET /api/v1/skills/{skill_id} - 获取 Skill 详情

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetSkillRequest, GetSkillResponse};

use super::response::to_detail;

/// Get detailed information about a specific skill including metadata and file list
#[register_handler_tool(
    id = "get_skill",
    name = "get_skill",
    description = "Get detailed information about a specific skill including metadata and file list",
    params = "common::api::GetSkillRequest"
)]
#[generate_http_handler]
pub async fn get_skill(
    ctx: RequestContext,
    params: GetSkillRequest,
) -> Result<GetSkillResponse, AppError> {
    let skill = domain()
        .skill_manage()
        .get_skill(ctx, &params.skill_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Skill {} not found", params.skill_id)))?;

    Ok(to_detail(&skill))
}
