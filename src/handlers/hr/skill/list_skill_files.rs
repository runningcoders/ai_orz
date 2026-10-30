//! Handler: GET /api/v1/skills/{skill_id}/files - 列出 Skill 所有文件

use crate::models::skill::SKILL_MAIN_FILE;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListSkillFilesRequest, ListSkillFilesResponse};
use common::error::{Result, bail_err};

/// List all files in a skill with their absolute paths
#[register_handler_tool(
    id = "list_skill_files",
    name = "List Skill Files",
    description = "List all files inside a skill with filename, size and absolute path (abs_path). Use this to discover a skill's bundled assets: the root skill.md is the entry guide (read it with the get_skill tool, it is the only file served through the API), all other attachments must be accessed by their abs_path — read them with the read_file tool, and run bundled scripts with the shell tool. Fails with NotFound if the skill does not exist.",
    params = "common::api::ListSkillFilesRequest",
    tags = "skill_management"
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
                    filename: f.filename.clone(),
                    file_size: f.file_size,
                    is_main: f.filename.eq_ignore_ascii_case(SKILL_MAIN_FILE),
                    abs_path: f.abs_path.to_string_lossy().to_string(),
                    executable: f.executable,
                })
                .collect();
            Ok(ListSkillFilesResponse { files })
        }
    }
}
