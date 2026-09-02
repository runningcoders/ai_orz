//! Handler: POST /api/v1/skills/{skill_id}/restore
//!
//! 将一个 Expired 状态的 Skill 恢复为 Draft（详情页「过期技能虚拟 pack」
//! 中每条技能的「恢复」按钮调用）。只有当前状态=Expired 时允许操作；
//! 权限复用 ensure_skill_access：作者本人 / Admin / SuperAdmin /
//! （作者为 Agent 时）该 Agent 的创建者 均可执行。

use crate::handlers::hr::skill::response::to_detail;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{RestoreSkillRequest, RestoreSkillResponse};
use common::error::Result;

#[register_handler_tool(
    id = "restore_skill",
    name = "Restore Expired Skill",
    description = "Restore an Expired skill back to Draft so it can be edited and used again. Fails with Conflict if the skill is not currently Expired — check its status first. Returns the updated skill detail.",
    params = "common::api::RestoreSkillRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn restore_skill(
    ctx: RequestContext,
    params: RestoreSkillRequest,
) -> Result<RestoreSkillResponse> {
    let skill = domain()
        .skill_manage()
        .restore_skill(ctx, &params.skill_id)
        .await?;
    Ok(to_detail(&skill))
}
