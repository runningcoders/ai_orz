//! Handler: DELETE /api/v1/skills/{skill_id} - 删除 Skill

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::DeleteSkillRequest;
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

/// Delete an existing skill by ID. This operation cannot be undone.
#[register_handler_tool(
    id = "delete_skill",
    name = "delete_skill",
    description = "Delete an existing skill by ID. This operation cannot be undone.",
    params = "common::api::DeleteSkillRequest"
)]
#[generate_http_handler]
pub async fn delete_skill(
    ctx: RequestContext,
    params: DeleteSkillRequest,
) -> Result<(), AppError> {
    domain()
        .skill_manage()
        .get_skill(ctx.clone(), &params.skill_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Skill {} not found", params.skill_id)))?;

    domain().skill_manage().delete_skill(ctx, &params.skill_id).await?;

    Ok(())
}