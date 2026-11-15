//! Handler: 驳回收尾（项目 Owner / 发起者）打回执行人返工

use super::confirm_task::append_note;
use super::review_guard::ensure_reviewer;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{RejectTaskParams, RejectTaskResponse};
use common::enums::TaskStatus;
use common::error::Result;

/// 驳回任务，打回进行中
#[register_handler_tool(
    id = "reject_task",
    name = "Reject Task Review",
    description = "As the project owner or task initiator, reject a task that is PendingReview and send it back to InProgress for rework. A concrete reason is required and will be appended to the task execution result so the assignee knows what to fix. No completion delivery or DAG successor dispatch happens until the task is later confirmed.",
    params = "common::api::RejectTaskParams",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn reject_task(
    ctx: RequestContext,
    params: RejectTaskParams,
) -> Result<RejectTaskResponse> {
    if params.reason.trim().is_empty() {
        return Err(common::error::err!(
            InvalidRequest,
            "驳回原因不能为空，执行人需要据此返工"
        ));
    }

    let project_domain = project::domain();
    let mut task = project_domain
        .task_manage()
        .get(ctx.clone(), &params.task_id)
        .await?
        .ok_or_else(|| common::error::err!(NotFound, "Task {} not found", params.task_id))?;

    ensure_reviewer(&ctx, &task).await?;

    if task.po.status != TaskStatus::PendingReview {
        return Err(common::error::err!(
            InvalidRequest,
            "任务 {} 当前状态为 {:?}，只有待验收状态可以驳回",
            task.po.id,
            task.po.status
        ));
    }

    append_note(&mut task, "【验收驳回，请返工】", &params.reason);

    project_domain
        .task_manage()
        .transition_status(ctx, &mut task, TaskStatus::InProgress)
        .await?;

    Ok(RejectTaskResponse {
        task_id: params.task_id,
        status: "in_progress".to_string(),
    })
}
