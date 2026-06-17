//! 创建 Task

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
};
use common::api::{ApiResponse, CreateTaskRequest, CreateTaskResponse};
use common::enums::AssigneeType;

use super::response;

/// 创建 Task
/// POST /tasks
pub async fn create_task(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateTaskResponse>>), AppError> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        return Err(AppError::BadRequest("当前用户不能为空".to_string()));
    }
    if req.title.trim().is_empty() {
        return Err(AppError::BadRequest("任务标题不能为空".to_string()));
    }
    if req.assignee_id.trim().is_empty() {
        return Err(AppError::BadRequest("assignee_id 不能为空".to_string()));
    }

    let task = domain()
        .task_manage()
        .create_with_options(
            ctx,
            req.title,
            req.description.unwrap_or_default(),
            req.priority.unwrap_or_default(),
            req.tags.unwrap_or_default(),
            req.root_user_id.unwrap_or_else(|| current_user_id.clone()),
            req.assignee_type.unwrap_or(AssigneeType::Agent),
            req.assignee_id,
            req.project_id,
            req.due_at,
            req.dependencies.unwrap_or_default(),
            current_user_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(response::to_detail(&task))),
    ))
}
