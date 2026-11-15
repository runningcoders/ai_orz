//! 任务验收链路消费者（AOP **Sync**）
//!
//! 订阅 `task.status_changed`，处理两种转换：
//!
//! 1. `→ PendingReview`：给项目 Owner（owner → initiator fallback）发一条
//!    `TaskPendingReview(15)` 工作单，正文带验收标准 / 提交说明 / 产物清单 /
//!    验收动作指引。⚠️ 此时**不**发交付、不碰 DAG —— 那两件事只认 `Completed`。
//! 2. `PendingReview → InProgress`（驳回）：给执行 Agent 发一条返工通知。
//!
//! # 为什么是 Sync
//!
//! 与 `TaskDispatchConsumer` 同理：状态落库已是事实，通知若押在 Async 内存队列上，
//! 重启即丢，任务会**永久停在待验收且没有任何报错**。Sync 让 `aop::publish`
//! 返回时通知已落库。
//!
//! # 无 Owner 兜底
//!
//! 用户直接建任务 / 存量任务可能既无 `owner_agent_id` 也无 `initiator_agent_id`，
//! 此时没有验收责任人，必须就地自动确认（走 `transition_status → Completed`），
//! 否则任务永远无人推进。自动确认产生的第二次状态事件是 `Completed`，会被本
//! 消费者的两个分支都过滤掉，不会递归。
//!
//! # 失败口径
//!
//! 沿用完成交付的红线：状态写入是事实，投递失败只记日志并返回 `Ok(())`，
//! 不把状态事件打回重试（重试会重复发通知）。

use async_trait::async_trait;
use common::api::artifact::ArtifactDetail;
use common::enums::EventTopic;
use common::enums::message::{MessageRole, MessageType};
use common::enums::task::TaskStatus;
use common::error::Result;

use crate::models::events::TaskStatusChangedEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::service::dal::task::TaskFetchOptions;
use crate::service::domain::message::builder::{TaskReviewView, build_task_pending_review_content};
use crate::service::domain::message::{self, SendToAgentCommand};

pub struct TaskPendingReviewConsumer;

/// 选验收责任人：项目 Owner 优先、发起者兜底，两者都空 = 无责任人（自动确认）。
///
/// ⚠️ 空串必须当「无」：存量任务 / 用户直接建任务的 `owner_agent_id`、`
/// initiator_agent_id` 都可能是空串，把它们当有效 Agent 会发出一条谁也收不到
/// 的死信，任务却再也不会被自动确认。
fn pick_reviewer<'a>(
    owner_agent_id: Option<&'a str>,
    initiator_agent_id: &'a str,
) -> Option<&'a str> {
    owner_agent_id
        .filter(|s| !s.trim().is_empty())
        .or_else(|| (!initiator_agent_id.trim().is_empty()).then_some(initiator_agent_id))
}

impl TaskPendingReviewConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskPendingReviewConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for TaskPendingReviewConsumer {
    fn name(&self) -> &str {
        "task_pending_review_delivery"
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        // ⚠️ Sync 消费者声明顺序消费是谎言，注册期直接拒。
        vec![Subscription::new(EventTopic::TaskStatusChanged)]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: TaskStatusChangedEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!(
                "failed to deserialize TaskStatusChangedEvent: {e}"
            ))
        })?;

        let submitted = event.new_status == TaskStatus::PendingReview;
        let rejected = event.old_status == TaskStatus::PendingReview
            && event.new_status == TaskStatus::InProgress;
        if !submitted && !rejected {
            return Ok(());
        }

        // ① 项目行只查一次：owner（验收责任人）+ 旧事件缺快照时的归属用户回退。
        // 查询失败不当作「无项目」，只打日志并继续用事件快照身份。
        let project = match event.project_id.as_deref() {
            Some(pid) => match crate::service::domain::project::domain()
                .project_manage()
                .get(ctx.clone(), pid)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "task_pending_review_delivery",
                        project_id = %pid,
                        error = ?e,
                        "查询项目失败，验收责任人退回事件快照身份"
                    );
                    None
                }
            },
            None => None,
        };

        // ⚠️ 滚动升级残留的旧事件不带 `root_user_id`：必须回落到项目行，否则被唤醒的
        // Owner 拿不到 `ctx.user_id`（工具凭据 / 用户画像全断）。与完成交付消费者同口径。
        let root_user_id = if event.root_user_id.is_empty() {
            project
                .as_ref()
                .map(|p| p.po.root_user_id.as_str())
                .unwrap_or_default()
        } else {
            event.root_user_id.as_str()
        };
        let ctx = crate::consumer::enrich_org_from_project_user(&ctx, root_user_id).await;

        let owner_agent_id = project
            .as_ref()
            .and_then(|p| p.po.owner_agent_id.as_deref());
        let reviewer_id = pick_reviewer(owner_agent_id, &event.initiator_agent_id);

        let task = match crate::service::domain::project::domain()
            .task_manage()
            .get_task(
                ctx.clone(),
                &event.task_id,
                TaskFetchOptions {
                    with_stats: None,
                    with_model_call_stats: None,
                    stats_time_range: None,
                    stats_interval: None,
                    with_artifacts: Some(true),
                },
            )
            .await
        {
            Ok(t) => t,
            Err(e) => {
                log_warn!(
                    &ctx,
                    "task_pending_review_delivery",
                    task_id = %event.task_id,
                    error = ?e,
                    "读取任务验收材料失败，通知按空值降级"
                );
                None
            }
        };

        let reply_to_id =
            (!event.trigger_message_id.is_empty()).then_some(event.trigger_message_id.as_str());
        let message_domain = message::domain();

        if rejected {
            let content = format!(
                "【验收驳回，请返工】任务「{}」（`{}`）未通过验收，已打回进行中。\n\
                 驳回原因与验收结论记录在任务详情的执行结果里，请用 \
                 `get_task(task_id=\"{}\", with_artifacts=true)` 核对后继续。\n\
                 补齐后重新调用 `mark_done` 提交验收；在此之前不会触发完成交付，也不会解锁后继任务。",
                event.task_title, event.task_id, event.task_id,
            );
            // ⚠️ 不能用 `MessageRole::System`：非 ToolCallRequest 的 System 消息在
            // `handle_system_message` 里被直接丢弃、根本不唤醒执行人，返工会石沉大海。
            // 用 Agent 来源 + `AgentNotify`：既走 `handle_agent_message` 唤醒，又由
            // `agent_notify` 规则把 Final Discard，不会形成「返工通知 → 回复 → 再催」乒乓。
            let from_id = reviewer_id
                .filter(|id| *id != event.assignee_id.as_str())
                .unwrap_or("system");
            let cmd = SendToAgentCommand {
                from_id,
                from_role: MessageRole::Agent,
                to_agent_id: &event.assignee_id,
                content: &content,
                project_id: event.project_id.as_deref(),
                task_id: Some(&event.task_id),
                reply_to_id,
                external: Default::default(),
                attachment_ids: None,
                message_type: MessageType::AgentNotify,
            };
            if let Err(e) = message_domain
                .delivery()
                .send_to_agent(ctx.clone(), cmd)
                .await
            {
                log_warn!(
                    &ctx,
                    "task_pending_review_delivery",
                    task_id = %event.task_id,
                    error = ?e,
                    "验收驳回通知投递失败"
                );
            }
            return Ok(());
        }

        // 无验收责任人 ⇒ 自动确认，避免任务永久停在待验收。
        // 产生的 Completed 事件由既有完成交付 / DAG 消费者接走。
        let Some(reviewer_id) = reviewer_id else {
            let Some(mut task) = task.filter(|t| t.po.status == TaskStatus::PendingReview) else {
                return Ok(());
            };
            if let Err(e) = crate::service::domain::project::domain()
                .task_manage()
                .transition_status(ctx.clone(), &mut task, TaskStatus::Completed)
                .await
            {
                log_warn!(
                    &ctx,
                    "task_pending_review_delivery",
                    task_id = %event.task_id,
                    error = ?e,
                    "无验收责任人时自动确认失败"
                );
            }
            return Ok(());
        };

        let execution_result = task.as_ref().and_then(|t| t.po.execution_result.as_deref());
        let artifacts: Vec<ArtifactDetail> = task
            .as_ref()
            .and_then(|t| t.artifacts.clone())
            .unwrap_or_default();
        let acceptance_criteria = task
            .as_ref()
            .map(|t| t.po.acceptance_criteria.as_str())
            .unwrap_or_default();

        let content = build_task_pending_review_content(&TaskReviewView {
            task_id: &event.task_id,
            task_title: &event.task_title,
            project_id: event.project_id.as_deref(),
            assignee_id: &event.assignee_id,
            root_user_id,
            acceptance_criteria,
            execution_result,
            artifacts: &artifacts,
        });

        // from 取执行 Agent：Owner 看到的是「谁交上来的」，且回复义务判据
        // 因此成立；ReplyObligation 对本类型返回 NotifyOnly，不会形成乒乓。
        let from_id = if event.assignee_id.is_empty() {
            "system"
        } else {
            event.assignee_id.as_str()
        };
        let cmd = SendToAgentCommand {
            from_id,
            from_role: MessageRole::Agent,
            to_agent_id: reviewer_id,
            content: &content,
            project_id: event.project_id.as_deref(),
            task_id: Some(&event.task_id),
            reply_to_id,
            external: Default::default(),
            attachment_ids: None,
            message_type: MessageType::TaskPendingReview,
        };
        if let Err(e) = message_domain
            .delivery()
            .send_to_agent(ctx.clone(), cmd)
            .await
        {
            log_warn!(
                &ctx,
                "task_pending_review_delivery",
                task_id = %event.task_id,
                agent_id = %reviewer_id,
                error = ?e,
                "待验收通知投递失败"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reviewer_means_auto_confirm_path() {
        assert_eq!(pick_reviewer(None, ""), None);
        assert_eq!(pick_reviewer(Some(""), ""), None);
        assert_eq!(pick_reviewer(Some("   "), ""), None);
    }

    #[test]
    fn owner_takes_priority_over_initiator() {
        assert_eq!(
            pick_reviewer(Some("agent-owner"), "agent-reception"),
            Some("agent-owner")
        );
    }

    #[test]
    fn empty_owner_falls_back_to_initiator() {
        assert_eq!(
            pick_reviewer(Some(""), "agent-reception"),
            Some("agent-reception")
        );
        assert_eq!(
            pick_reviewer(None, "agent-reception"),
            Some("agent-reception")
        );
    }
}
