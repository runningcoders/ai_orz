//! Handler: 确认验收通过（项目 Owner / 发起者）
//!
//! 这是任务生命周期里**唯一**把状态推进到 `Completed` 的工具入口。
//! `TaskStatusChangedEvent` 随后由 Sync 的完成交付消费者与 Async 的 DAG
//! 后继补发消费者各自接走 —— 两者都以 `new_status == Completed` 为门禁。

use super::review_guard::ensure_reviewer;
use crate::models::task::Task;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ConfirmTaskParams, ConfirmTaskResponse};
use common::enums::TaskStatus;
use common::error::Result;

/// 确认任务验收通过
#[register_handler_tool(
    id = "confirm_task",
    name = "Confirm Task Review",
    description = "As the project owner or task initiator, accept a task that is PendingReview and mark it Completed. This is the only action that triggers the completion delivery to the root user and unlocks dependent DAG tasks. Only valid from PendingReview; calling it again on an already completed task is a no-op. Use reject_task instead when the deliverables do not meet the acceptance criteria.",
    params = "common::api::ConfirmTaskParams",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn confirm_task(
    ctx: RequestContext,
    params: ConfirmTaskParams,
) -> Result<ConfirmTaskResponse> {
    let project_domain = project::domain();
    let mut task = project_domain
        .task_manage()
        .get(ctx.clone(), &params.task_id)
        .await?
        .ok_or_else(|| common::error::err!(NotFound, "Task {} not found", params.task_id))?;

    ensure_reviewer(&ctx, &task).await?;

    // 幂等：重复确认不产生第二次状态事件（否则交付消息 + DAG 补发各来一份）。
    // ⚠️ 已知边界：这是「读时状态」判据，两个验收人**并发**确认仍可能各读到
    // PendingReview 各写一次。串行重复调用已被挡住；真正的并发 CAS 需要在
    // `TaskDalImpl::update` 引入 `WHERE status = ?` 的原子占坑，属独立一轮
    // 基础设施改造（牵动状态事件唯一发布点），本次按 YAGNI 延后。
    // TODO(并发验收): 见 docs/modules/05-任务与项目/A-设计/task_acceptance_and_dual_channel_design.md
    // 的「已知边界」——需要 CAS 时改这里与 `TaskDalImpl::update`，不要只在本函数加重试。
    if task.po.status == TaskStatus::Completed {
        return Ok(ConfirmTaskResponse {
            task_id: params.task_id,
            status: "completed".to_string(),
        });
    }
    if task.po.status != TaskStatus::PendingReview {
        return Err(common::error::err!(
            InvalidRequest,
            "任务 {} 当前状态为 {:?}，只有待验收状态可以确认通过；执行人需先调用 mark_done 提交",
            task.po.id,
            task.po.status
        ));
    }

    if let Some(note) = params.note.as_deref().filter(|s| !s.trim().is_empty()) {
        append_note(&mut task, "【验收结论】", note);
    }

    project_domain
        .task_manage()
        .transition_status(ctx, &mut task, TaskStatus::Completed)
        .await?;

    Ok(ConfirmTaskResponse {
        task_id: params.task_id,
        status: "completed".to_string(),
    })
}

/// 验收结论追加到执行结果，不覆盖执行人自己的提交说明
pub(super) fn append_note(task: &mut Task, label: &str, text: &str) {
    let prev = task.po.execution_result.clone().unwrap_or_default();
    task.po.execution_result = Some(if prev.is_empty() {
        format!("{label}{text}")
    } else {
        format!("{prev}\n\n{label}{text}")
    });
}
