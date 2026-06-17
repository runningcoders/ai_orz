//! Handler: GET /api/v1/skills/{skill_id}/files/{*filename} - 读取 Skill 文件内容

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{GetSkillFileContentParams, GetSkillFileContentResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

/// Get the content of a file in a skill
#[register_handler_tool(
    id = "get_skill_file_content",
    name = "get_skill_file_content",
    description = "Read the text content of a specific file from a skill",
    params = "common::api::GetSkillFileContentParams"
)]
#[generate_http_handler]
pub async fn get_skill_file_content(
    ctx: RequestContext,
    params: GetSkillFileContentParams,
) -> Result<GetSkillFileContentResponse, AppError> {
    let result = domain()
        .skill_manage()
        .get_skill_file_content(ctx, &params.skill_id, &params.filename)
        .await?;

    match result {
        None => Err(AppError::NotFound(format!(
            "Skill file not found: {}/{}",
            params.skill_id, params.filename
        ))),
        Some(content) => Ok(content),
    }
}