//! Handler: 标记任务完成

use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{MarkDoneParams, MarkDoneResponse};
use common::error::Result;

/// 标记任务完成
#[register_handler_tool(
    id = "mark_done",
    name = "mark_done",
    description = "Mark a task as completed by task_id. Performs state transition to Completed state; fails if the task is in a non-completable state. Use this when a task's work is finished.",
    params = "common::api::MarkDoneParams",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn mark_done(ctx: RequestContext, params: MarkDoneParams) -> Result<MarkDoneResponse> {
    let project_domain = project::domain();

    let task = project_domain
        .task_manage()
        .get(ctx.clone(), &params.task_id)
        .await?
        .ok_or_else(|| common::error::err!(NotFound, "Task {} not found", params.task_id))?;

    let modified_by = ctx
        .agent_id()
        .cloned()
        .or_else(|| ctx.user_id().cloned())
        .unwrap_or_else(|| "system".to_string());

    project_domain
        .task_manage()
        .complete(ctx, &task.po.id, modified_by)
        .await?;

    Ok(MarkDoneResponse {
        task_id: params.task_id,
        status: "completed".to_string(),
    })
}
