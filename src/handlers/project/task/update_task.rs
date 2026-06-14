//! 更新 Task 基础信息

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, UpdateTaskRequest, UpdateTaskResponse};

use super::response;

/// 更新 Task 基础信息
/// PUT /tasks/{id}
pub async fn update_task(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<AxumJson<ApiResponse<UpdateTaskResponse>>, AppError> {
    let task = domain()
        .task()
        .update_basic(
            ctx,
            &id,
            req.title,
            req.description,
            req.priority,
            req.tags,
            req.due_at,
            req.dependencies,
        )
        .await?;

    Ok(AxumJson(ApiResponse::success(response::to_detail(&task))))
}
