//! 更新 Task 状态

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, UpdateTaskStatusRequest, UpdateTaskStatusResponse};

use super::response;

/// 更新 Task 状态
/// PUT /tasks/{id}/status
pub async fn update_task_status(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskStatusRequest>,
) -> Result<AxumJson<ApiResponse<UpdateTaskStatusResponse>>, AppError> {
    let mut task = domain()
        .task_manage()
        .get(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {} not found", id)))?;

    domain()
        .task_manage()
        .transition_status(ctx, &mut task, req.status)
        .await?;

    Ok(AxumJson(ApiResponse::success(response::to_detail(&task))))
}
