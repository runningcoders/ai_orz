//! Handler: register_artifact_from_path - Register a file as artifact

use super::mime_util;
use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::artifact::{ArtifactDetail, RegisterArtifactFromPathParams};
use common::error::{Result, bail_err, err};

/// Register an existing file (in agent's directory) as an artifact.
///
/// The file will be **copied** to artifact storage. Source file is preserved
/// so the agent can continue working on it.
#[register_handler_tool(
    id = "register_artifact_from_path",
    name = "register_artifact_from_path",
    description = "Register an existing file (in agent's directory) as an artifact. The file will be copied to artifact storage.",
    params = "common::api::RegisterArtifactFromPathParams",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn register_artifact_from_path(
    ctx: RequestContext,
    params: RegisterArtifactFromPathParams,
) -> Result<ArtifactDetail> {
    let agent_id = ctx
        .agent_id()
        .ok_or_else(|| err!(InvalidRequest, "agent_id is required for register_artifact_from_path"))?;

    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.project_id.trim().is_empty() {
        bail_err!(InvalidRequest, "project_id不能为空");
    }
    if params.name.trim().is_empty() {
        bail_err!(InvalidRequest, "name不能为空");
    }
    if params.source_path.trim().is_empty() {
        bail_err!(InvalidRequest, "source_path不能为空");
    }

    // Compute source file absolute path
    let agent_dir = crate::config::get().agent_data_dir(agent_id);
    let source_path = agent_dir.join(&params.source_path);

    // Security: source path must be under agent's directory (prevent traversal)
    let source_canonical = source_path
        .canonicalize()
        .map_err(|_| err!(InvalidRequest, "源文件不存在或无法访问: {}", params.source_path))?;
    let agent_dir_canonical = agent_dir
        .canonicalize()
        .map_err(|_| err!(Internal, "Agent directory not accessible"))?;
    if !source_canonical.starts_with(&agent_dir_canonical) {
        bail_err!(InvalidRequest, "source_path 越界：必须在 agent 目录之下");
    }

    // Validate source file exists and is a file
    let file_metadata = std::fs::metadata(&source_canonical)
        .map_err(|_| err!(InvalidRequest, "源文件不存在: {}", params.source_path))?;
    if !file_metadata.is_file() {
        bail_err!(InvalidRequest, "source_path 不是文件: {}", params.source_path);
    }

    // Derive file_name and mime_type
    let file_name = params
        .file_name
        .unwrap_or_else(|| mime_util::basename(&params.source_path));
    let mime_type = params
        .mime_type
        .unwrap_or_else(|| mime_util::infer_mime_type(&file_name));
    let file_type = params
        .file_type
        .unwrap_or_else(|| mime_util::infer_file_type(&mime_type));

    let artifact = project::domain()
        .artifact_manage()
        .create_generated_artifact_from_file(
            ctx,
            params.project_id,
            params.task_id,
            params.name,
            params.description.unwrap_or_default(),
            source_canonical,
            file_name,
            mime_type,
            file_type,
            params.tags.unwrap_or_default(),
            current_user_id,
        )
        .await?;

    Ok(response::to_detail(&artifact))
}
