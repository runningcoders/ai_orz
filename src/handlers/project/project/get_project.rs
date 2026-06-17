//! 获取 Project

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, GetProjectResponse};

use super::response;

/// 获取 Project
/// GET /projects/{id}
pub async fn get_project(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetProjectResponse>>, AppError> {
    let project = domain()
        .project_manage()
        .get(ctx, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Project {} not found", id)))?;

    Ok(Json(ApiResponse::success(response::to_detail(&project))))
}
