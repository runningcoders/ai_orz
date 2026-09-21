//! Handler: PUT /api/v1/tasks/{id} - Update task basic information

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateTaskRequest, UpdateTaskResponse};
use common::error::Result;

/// Update task basic information (title, description, priority, tags, etc.)
///
/// 挂载语义：请求携带 project_id 且任务尚未挂载任何项目时，将任务挂载到该项目；
/// 任务已挂载项目时忽略该参数（第一版不支持迁移/解绑）。
#[register_handler_tool(
    id = "update_task",
    name = "Update Task",
    description = "Partially update a task's basic info: title, description, priority, tags, due date, dependency task IDs, execution plan, and execution result; only provided fields change. If project_id is provided and the task is not yet bound to any project, the task is also bound to that project; if the task already has a project, the project_id parameter is ignored. This does not change status or progress — use update_task_status or update_task_progress for those.",
    params = "common::api::UpdateTaskRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_task(
    ctx: RequestContext,
    params: UpdateTaskRequest,
) -> Result<UpdateTaskResponse> {
    let domain = domain();
    let task_manage = domain.task_manage();

    if let Some(project_id) = params.project_id.clone()
        && let Some(task) = task_manage.get(ctx.clone(), &params.id).await?
        && task.po.project_id.is_none()
    {
        task_manage
            .bind_to_project(ctx.clone(), &params.id, project_id)
            .await?;
    }

    let task = task_manage
        .update_basic(
            ctx,
            &params.id,
            params.title,
            params.description,
            params.priority,
            params.tags,
            params.due_at,
            params.dependencies,
            params.execution_plan,
            params.execution_result,
        )
        .await?;

    Ok(response::to_detail(&task))
}
