//! Handler: PUT /api/v1/tasks/{id}/status - Update task status (state transition)

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateTaskStatusRequest, UpdateTaskStatusResponse};
use common::enums::TaskStatus;
use common::error::Result;

use crate::enrich_ctx;

/// Update task status with state transition validation
#[register_handler_tool(
    id = "update_task_status",
    name = "Update Task Status",
    description = "Move a task through its validated state machine (Pending, InProgress, Archived). Starting sets the start timestamp. Agents must NOT use this tool to enter or leave PendingReview (Submit Task For Review / Confirm Task Review / Reject Task Review own those two edges) and cannot set Completed at all: the assignee calls mark_done to submit, then the project owner calls confirm_task. Returns the updated task detail. Fails on illegal transitions; cancellation is not allowed through this endpoint.",
    params = "common::api::UpdateTaskStatusRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_task_status(
    ctx: RequestContext,
    params: UpdateTaskStatusRequest,
) -> Result<UpdateTaskStatusResponse> {
    let task = domain()
        .task_manage()
        .get(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Task {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &task);
    let mut task = task;

    // Agent 工具面不得绕过验收：验收闭环的三个动作各有**唯一**入口 ——
    // 提交 `mark_done`（校验承接人）、确认 `confirm_task` / 驳回 `reject_task`
    // （两者都带 `ensure_reviewer`）。只拦 `Completed` 不够：那样 Agent 仍可自己
    // 提交（绕过承接人判定）、或把别人的待验收任务直接打回（绕过验收责任人判定，
    // 且不带驳回原因）。管理面（无 agent_id）保留管理员直接改状态的能力；A2A 回调
    // 走 Domain 的 transition_status，不经过这里，三者互不干扰。
    //
    // 同状态视为 no-op（Domain 内等值短路），不当作越权拦截。
    if let Some(caller) = ctx.agent_id()
        && params.status != task.po.status
    {
        match (task.po.status, params.status) {
            (_, TaskStatus::Completed) => {
                return Err(common::error::err!(
                    InvalidRequest,
                    "Agent 不能直接把任务置为 Completed：请先用 mark_done 提交验收，再由项目 Owner 用 confirm_task 确认"
                ));
            }
            (_, TaskStatus::PendingReview) => {
                return Err(common::error::err!(
                    InvalidRequest,
                    "Agent {} 不能通过状态接口把任务置为 PendingReview：只有任务承接人能提交验收，请调用 mark_done",
                    caller
                ));
            }
            (TaskStatus::PendingReview, TaskStatus::InProgress) => {
                return Err(common::error::err!(
                    InvalidRequest,
                    "任务处于待验收状态，Agent {} 不能直接打回：请由任务发起者 / 项目 Owner 用 reject_task（必填驳回原因）决定",
                    caller
                ));
            }
            _ => {}
        }
    }

    domain()
        .task_manage()
        .transition_status(ctx, &mut task, params.status)
        .await?;

    Ok(response::to_detail(&task))
}
