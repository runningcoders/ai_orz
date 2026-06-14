//! 创建 Project

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
};
use common::api::{ApiResponse, CreateProjectRequest, CreateProjectResponse};

use super::response;

/// 创建 Project
/// POST /projects
pub async fn create_project(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateProjectResponse>>), AppError> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        return Err(AppError::BadRequest("当前用户不能为空".to_string()));
    }

    let project = domain()
        .project()
        .create(
            ctx,
            req.name,
            req.description.unwrap_or_default(),
            req.priority.unwrap_or_default(),
            req.tags.unwrap_or_default(),
            current_user_id.clone(),
            current_user_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(response::to_detail(&project))),
    ))
}
