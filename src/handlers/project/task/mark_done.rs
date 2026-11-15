//! Handler: 提交任务验收（原「标记完成」）
//!
//! 2026-11-15 起「完成」拆成两个动作：执行人在这里**声明做完**（PendingReview），
//! 项目 Owner 用 `confirm_task` 确认后才转 `Completed` —— 后者才是唯一会触发
//! 完成交付消息与 DAG 后继补发的状态。

use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{MarkDoneParams, MarkDoneResponse};
use common::error::Result;

/// 提交任务验收
#[register_handler_tool(
    id = "mark_done",
    name = "Submit Task For Review",
    description = "Declare that your work on the task is finished and submit it for review. This moves the task to PendingReview (NOT Completed): the project owner will verify the acceptance criteria and confirm. Only the assigned executor may submit, and only from InProgress. The optional summary is persisted as the execution result and shown to the reviewer. Do NOT use update_task_status(Completed) to bypass review.",
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

    // 提交权属于承接人：工具包授权只说明「会用项目工具」，不说明「这个任务是你的」。
    if let Some(caller) = ctx.agent_id()
        && caller != &task.po.assignee_id
    {
        return Err(common::error::err!(
            InvalidRequest,
            "只有任务承接人 {} 可以提交验收，当前调用方 {}",
            task.po.assignee_id,
            caller
        ));
    }

    let modified_by = ctx.caller_id_or_system();
    project_domain
        .task_manage()
        .submit_for_review(ctx, &task.po.id, modified_by, params.summary.clone())
        .await?;

    Ok(MarkDoneResponse {
        task_id: params.task_id,
        status: "pending_review".to_string(),
    })
}
