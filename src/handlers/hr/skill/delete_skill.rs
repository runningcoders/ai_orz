//! Handler: DELETE /api/v1/skills/{skill_id} - 删除 Skill

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{DeleteSkillRequest, DeleteSkillResponse};
use common::error::Result;

/// Delete an existing skill by ID. This operation cannot be undone.
#[register_handler_tool(
    id = "delete_skill",
    name = "Remove Skill",
    description = "Permanently delete a skill by ID; this cannot be undone. If you may need it later, set its status to Expired instead and recover it with restore_skill. Returns success.",
    params = "common::api::DeleteSkillRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn delete_skill(
    ctx: RequestContext,
    params: DeleteSkillRequest,
) -> Result<DeleteSkillResponse> {
    domain()
        .skill_manage()
        .get_skill(ctx.clone(), &params.skill_id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("Skill {} not found", params.skill_id))
        })?;

    domain()
        .skill_manage()
        .delete_skill(ctx, &params.skill_id)
        .await?;

    // 协议规范：即使无业务字段也返回标准 Response 结构体，禁止裸 ()
    Ok(DeleteSkillResponse { success: true })
}
