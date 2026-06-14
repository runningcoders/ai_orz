//! 更新 Project 状态

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, UpdateProjectStatusRequest, UpdateProjectStatusResponse};

use super::response;

/// 更新 Project 状态
/// PUT /projects/{id}/status
pub async fn update_project_status(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectStatusRequest>,
) -> Result<AxumJson<ApiResponse<UpdateProjectStatusResponse>>, AppError> {
    let mut project = domain()
        .project()
        .get(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Project {} not found", id)))?;

    domain()
        .project()
        .transition_status(ctx, &mut project, req.status)
        .await?;

    Ok(AxumJson(ApiResponse::success(response::to_detail(
        &project,
    ))))
}
