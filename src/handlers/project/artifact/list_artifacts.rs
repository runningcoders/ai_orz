//! Handler: GET /api/v1/projects/{project_id}/artifacts - List artifacts under a project

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{ListArtifactsRequest, ListArtifactsResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::{self, ListArtifactsParams};
use super::response;

const DEFAULT_MAX_LIMIT: usize = 100;

/// List all artifacts under a specific project with optional filtering
#[register_handler_tool(
    id = "list_artifacts",
    name = "list_artifacts",
    description = "List all artifacts under a project, with optional filtering by task, file type, or source type",
    params = "common::api::ListArtifactsRequest",
)]
#[generate_http_handler]
pub async fn list_artifacts(
    ctx: RequestContext,
    params: ListArtifactsRequest,
) -> Result<ListArtifactsResponse, AppError> {
    if params.project_id.trim().is_empty() {
        return Err(AppError::BadRequest("project_id 不能为空".to_string()));
    }

    let artifacts = project::domain()
        .artifact_manage()
        .list(
            ctx,
            ListArtifactsParams {
                project_id: params.project_id,
                task_id: params.task_id,
                file_type: params.file_type,
                source_type: params.source_type,
                limit: Some(
                    params
                        .limit
                        .unwrap_or(DEFAULT_MAX_LIMIT)
                        .min(DEFAULT_MAX_LIMIT),
                ),
            },
        )
        .await?;
    let response_items = artifacts.iter().map(response::to_detail).collect();

    Ok(response_items)
}