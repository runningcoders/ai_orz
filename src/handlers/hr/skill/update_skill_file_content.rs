//! Handler: PUT /api/v1/skills/{skill_id}/files/{*filename} - 创建或更新 Skill 文件

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateSkillFileContentRequest, UpdateSkillFileContentResponse};
use common::error::Result;

/// Create a new file or update the content of an existing text file in a skill. If the file doesn't exist, it will be created. If it exists, it will be overwritten.
#[register_handler_tool(
    id = "update_skill_file_content",
    name = "Update Skill File",
    description = "Create or fully overwrite a single text file inside a skill, such as its main skill.md. Pass expected_updated_at (the skill's updated_at in seconds) for optimistic locking — a mismatch returns Conflict. Returns an empty confirmation.",
    params = "common::api::UpdateSkillFileContentRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn update_skill_file_content(
    ctx: RequestContext,
    params: UpdateSkillFileContentRequest,
) -> Result<UpdateSkillFileContentResponse> {
    domain()
        .skill_manage()
        .update_skill_file_content(
            ctx,
            &params.skill_id,
            &params.filename,
            &params.content,
            params.expected_updated_at,
        )
        .await?;

    Ok(UpdateSkillFileContentResponse {})
}
