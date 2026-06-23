//! Handler: POST /api/v1/tasks - Create a new task

use super::response;
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateTaskRequest, CreateTaskResponse};
use common::enums::AssigneeType;

/// Create a new task
#[register_handler_tool(
    id = "create_task",
    name = "create_task",
    description = "Create a new task with specified title, description, assignee, etc.",
    params = "common::api::CreateTaskRequest"
)]
#[generate_http_handler]
pub async fn create_task(
    ctx: RequestContext,
    params: CreateTaskRequest,
) -> Result<CreateTaskResponse, AppError> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        return Err(AppError::BadRequest("当前用户不能为空".to_string()));
    }
    if params.title.trim().is_empty() {
        return Err(AppError::BadRequest("任务标题不能为空".to_string()));
    }
    if params.assignee_id.trim().is_empty() {
        return Err(AppError::BadRequest("assignee_id 不能为空".to_string()));
    }

    let task = domain()
        .task_manage()
        .create_with_options(
            ctx,
            params.title,
            params.description.unwrap_or_default(),
            params.priority.unwrap_or_default(),
            params.tags.unwrap_or_default(),
            params
                .root_user_id
                .unwrap_or_else(|| current_user_id.clone()),
            params.assignee_type.unwrap_or(AssigneeType::Agent),
            params.assignee_id,
            params.project_id,
            params.due_at,
            params.dependencies.unwrap_or_default(),
            current_user_id,
        )
        .await?;

    Ok(response::to_detail(&task))
}
