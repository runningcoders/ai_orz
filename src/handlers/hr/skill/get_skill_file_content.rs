//! Handler: GET /api/v1/skills/{skill_id}/files/{*filename} - 读取 Skill 文件内容

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetSkillFileContentRequest, GetSkillFileContentResponse};
use common::error::{Result, bail_err};

/// Read the content of the root skill.md (the only file served through the API)
#[register_handler_tool(
    id = "get_skill_file_content",
    name = "Read Skill Main File",
    description = "Read the content of a skill's root skill.md — the entry guide that explains how to use the skill. This is the ONLY file served through the API: any other attachment is rejected with InvalidRequest, use list_skill_files to get its abs_path and read it with the read_file tool (or run it with the shell tool if it is a script).",
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
