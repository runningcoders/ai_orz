//! 更新 Project

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, UpdateProjectRequest, UpdateProjectResponse};

use super::response;

/// 更新 Project
/// PUT /projects/{id}
pub async fn update_project(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> Result<AxumJson<ApiResponse<UpdateProjectResponse>>, AppError> {
    let modified_by = ctx.uid();
    let project = domain()
        .project()
        .update_basic(
            ctx,
            &id,
            req.name,
            req.description,
            req.priority,
            req.tags,
            modified_by,
        )
        .await?;

    Ok(AxumJson(ApiResponse::success(response::to_detail(
        &project,
    ))))
}
