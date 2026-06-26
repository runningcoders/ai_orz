//! Handler: PUT /api/v1/skills/{skill_id}/files/{*filename} - 创建或更新 Skill 文件

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateSkillFileContentRequest, UpdateSkillFileContentResponse};

/// Create a new file or update the content of an existing text file in a skill. If the file doesn't exist, it will be created. If it exists, it will be overwritten.
#[register_handler_tool(
    id = "update_skill_file_content",
    name = "update_skill_file_content",
    description = "Create a new file or update the content of an existing text file in a skill. Supports optimistic locking with expected_updated_at.",
    params = "common::api::UpdateSkillFileContentRequest"
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
