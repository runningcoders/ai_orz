//! 获取 Task 详情

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, GetTaskResponse};

use super::response;

/// 获取 Task 详情
/// GET /tasks/{id}
pub async fn get_task(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetTaskResponse>>, AppError> {
    let task = domain()
        .task()
        .get(ctx, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {} not found", id)))?;

    Ok(Json(ApiResponse::success(response::to_detail(&task))))
}
