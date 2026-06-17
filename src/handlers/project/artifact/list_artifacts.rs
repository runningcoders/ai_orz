//! 列出 Artifact

use axum::{
    Json,
    extract::{Extension, Query},
};
use common::api::{ApiResponse, ListArtifactsQuery, ListArtifactsResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::{self, ListArtifactsParams};

use super::response;

const DEFAULT_MAX_LIMIT: usize = 100;

/// 列出 Artifact
/// GET /api/v1/project/artifacts
pub async fn list_artifacts(
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<ListArtifactsQuery>,
) -> Result<Json<ApiResponse<ListArtifactsResponse>>, AppError> {
    if query.project_id.trim().is_empty() {
        return Err(AppError::BadRequest("project_id 不能为空".to_string()));
    }

    let artifacts = project::domain()
        .artifact_manage()
        .list(
            ctx,
            ListArtifactsParams {
                project_id: query.project_id,
                task_id: query.task_id,
                file_type: query.file_type,
                source_type: query.source_type,
                limit: Some(
                    query
                        .limit
                        .unwrap_or(DEFAULT_MAX_LIMIT)
                        .min(DEFAULT_MAX_LIMIT),
                ),
            },
        )
        .await?;
    let response_items = artifacts.iter().map(response::to_detail).collect();

    Ok(Json(ApiResponse::success(response_items)))
}
