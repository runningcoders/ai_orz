//! Handler: GET /api/v1/projects/{project_id}/artifacts - List artifacts under a project

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::{self, ListArtifactsParams};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ArtifactDetail, ListArtifactsRequest, PagedResult};
use common::error::{Result, bail_err};

const DEFAULT_MAX_LIMIT: usize = 100;

/// List all artifacts under a specific project with optional filtering
#[register_handler_tool(
    id = "list_artifacts",
    name = "List Artifacts",
    description = "Browse all artifacts under a specific project with pagination (limit capped at 100, default 100), optionally filtered by task_id, file type, or source type. Returns a paged list of artifact details. For cross-project filtering use query_artifacts.",
    params = "common::api::ListArtifactsRequest"
)]
#[generate_http_handler]
pub async fn list_artifacts(
    ctx: RequestContext,
    params: ListArtifactsRequest,
) -> Result<PagedResult<ArtifactDetail>> {
    if params.project_id.trim().is_empty() {
        bail_err!(InvalidRequest, "project_id 不能为空");
    }

    let limit = params
        .limit
        .map(|l| l.min(DEFAULT_MAX_LIMIT))
        .or(Some(DEFAULT_MAX_LIMIT));

    let page = project::domain()
        .artifact_manage()
        .list(
            ctx,
            ListArtifactsParams {
                project_id: params.project_id,
                task_id: params.task_id,
                file_type: params.file_type,
                source_type: params.source_type,
                pagination: common::api::PaginationParams {
                    limit,
                    offset: params.offset,
                },
            },
        )
        .await?;

    Ok(page.map(|a| response::to_detail(&a)))
}
