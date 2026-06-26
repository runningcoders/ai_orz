//! Handler: GET /api/v1/skills/{skill_id}/files - 列出 Skill 所有文件

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListSkillFilesRequest, ListSkillFilesResponse};
use common::error::{Result, err, bail_err};

/// List all files in a skill with their metadata (filename, size)
#[register_handler_tool(
    id = "list_skill_files",
    name = "list_skill_files",
    description = "List all files in a skill with their metadata (filename, size)",
    params = "common::api::ListSkillFilesRequest"
)]
#[generate_http_handler]
pub async fn list_skill_files(
    ctx: RequestContext,
    params: ListSkillFilesRequest,
) -> Result<ListSkillFilesResponse> {
    let result = domain()
        .skill_manage()
        .list_skill_files(ctx, &params.skill_id)
        .await?;

    match result {
        None => {
            bail_err!(NotFound, "Skill not found: {}", params.skill_id);
        }
        Some(files) => {
            let files = files
                .into_iter()
                .map(|f| common::api::SkillFileItem {
                    filename: f.filename,
                    file_size: f.file_size,
                    has_content: f.content.is_some(),
                })
                .collect();
            Ok(ListSkillFilesResponse { files })
        }
    }
}