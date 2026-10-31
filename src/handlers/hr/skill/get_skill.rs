//! Handler: GET /api/v1/skills/{skill_id} - 获取 Skill 详情

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetSkillRequest, GetSkillResponse};
use common::error::Result;

use super::response::to_detail;

/// Get detailed information about a specific skill including metadata and file list
#[register_handler_tool(
    id = "get_skill",
    name = "Get Skill Details",
    description = "Get one skill's full detail by ID, including metadata (name, description, tags, category, status), the main skill.md content, and its file list. Fails with NotFound if the skill does not exist.",
    params = "common::api::GetSkillRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn get_skill(ctx: RequestContext, params: GetSkillRequest) -> Result<GetSkillResponse> {
    // 角色要在 ctx 被消费前取出来（详情 DTO 需要它判定「发布」按钮是否显示）
    let can_publish = crate::models::skill::can_publish_by_role(ctx.user_role());
    let skill = domain()
        .skill_manage()
        .get_skill(ctx, &params.skill_id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("Skill {} not found", params.skill_id))
        })?;

    let mut detail = to_detail(&skill);
    // 与 `publish_skill` 的领域校验同源（`can_publish_by_role`），前端只按它显隐按钮。
    detail.can_publish = can_publish;
    Ok(detail)
}
