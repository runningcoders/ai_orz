//! Handler: POST /api/v1/tasks - Create a new task

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use crate::service::domain::message::{self, SendTaskAssignmentCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateTaskRequest, CreateTaskResponse};
use common::enums::{AssigneeType, MessageRole};
use common::error::{Result, err, bail_err};

use crate::enrich_ctx;

/// Create a new task
#[register_handler_tool(
    id = "create_task",
    name = "create_task",
    description = "Create a new task with specified title, description, assignee, etc.",
    params = "common::api::CreateTaskRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn create_task(
    ctx: RequestContext,
    params: CreateTaskRequest,
) -> Result<CreateTaskResponse> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.title.trim().is_empty() {
        bail_err!(InvalidRequest, "task title 不能为空");
    }
    if params.assignee_id.trim().is_empty() {
        bail_err!(InvalidRequest, "assignee_id 不能为空");
    }

    let ctx = ctx.to_builder().try_project_id(params.project_id.as_deref()).build();

    let assignee_type = params.assignee_type.unwrap_or(AssigneeType::Agent);
    let assignee_id = params.assignee_id.clone();
    let task_description = params.description.clone();

    let task = domain()
        .task_manage()
        .create_with_options(
            ctx.clone(),
            params.title.clone(),
            task_description.clone().unwrap_or_default(),
            params.priority.unwrap_or_default(),
            params.tags.unwrap_or_default(),
            params
                .root_user_id
                .unwrap_or_else(|| current_user_id.clone()),
            assignee_type,
            assignee_id.clone(),
            params.project_id.clone(),
            params.due_at,
            params.dependencies.unwrap_or_default(),
            current_user_id.clone(),
        )
        .await?;

    // 如果分配给 Agent，发送任务分配通知消息
    // Project Domain 只负责数据持久化，通知由 Message Domain 负责
    if assignee_type == AssigneeType::Agent {
        let cmd = SendTaskAssignmentCommand {
            task_id: &task.id(),
            task_title: &task.po.title,
            task_description: task_description.as_deref(),
            from_id: &current_user_id,
            from_role: MessageRole::User,
            to_agent_id: &assignee_id,
            project_id: params.project_id.as_deref(),
        };

        let _ = message::domain()
            .delivery()
            .send_task_assignment(ctx, cmd)
            .await;
    }

    Ok(response::to_detail(&task))
}