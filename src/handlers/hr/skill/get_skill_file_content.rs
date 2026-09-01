//! Handler: GET /api/v1/skills/{skill_id}/files/{*filename} - 读取 Skill 文件内容

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetSkillFileContentRequest, GetSkillFileContentResponse};
use common::error::{Result, bail_err};

/// Read the text content of a specific file from a skill
#[register_handler_tool(
    id = "get_skill_file_content",
    name = "Read Skill File Content",
    description = "Read the text content of a specific file from a skill",
    params = "common::api::GetSkillFileContentRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn get_skill_file_content(
    ctx: RequestContext,
    params: GetSkillFileContentRequest,
) -> Result<GetSkillFileContentResponse> {
    let result = domain()
        .skill_manage()
        .get_skill_file_content(ctx, &params.skill_id, &params.filename)
        .await?;

    match result {
        None => {
            bail_err!(
                NotFound,
                "Skill file not found: {}/{}",
                params.skill_id,
                params.filename
            );
        }
        Some(content) => Ok(GetSkillFileContentResponse { content }),
    }
}
