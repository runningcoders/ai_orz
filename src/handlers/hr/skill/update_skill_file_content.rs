//! Handler: PUT /api/v1/skills/{skill_id}/files/{*filename} - 创建或更新 Skill 文件

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{UpdateSkillFileContentParams, UpdateSkillFileContentResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

/// Create or update a text file in a skill
#[register_handler_tool(
    id = "update_skill_file_content",
    name = "update_skill_file_content",
    description = "Create a new file or update the content of an existing text file in a skill. Supports optimistic locking with expected_updated_at to prevent conflicts.",
    params = "common::api::UpdateSkillFileContentParams"
)]
#[generate_http_handler]
pub async fn update_skill_file_content(
    ctx: RequestContext,
    params: UpdateSkillFileContentParams,
) -> Result<UpdateSkillFileContentResponse, AppError> {
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