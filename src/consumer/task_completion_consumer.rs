//! 任务完成交付消费者（AOP **Sync**）
//!
//! 订阅 `task.status.changed`，仅在 `new_status == Completed` 时产出**唯一交付载体**：
//!
//! 1. 一条 `TaskCompleted(14)` 交付消息**直达 `root_user_id`**：正文带任务维度关键字段、
//!    `execution_result` 摘要、关联产物清单（`[名称](artifact:<id>)` 协议）
//! 2. 一条 `AgentNotify` 知会给任务发起者 / 项目 owner：只说明「交付已直达用户」，
//!    `NotifyOnly` + 静态规则 Discard 双保险，不产生第二份结果
//!
//! # 知会按项目合并
//!
//! 交付消息**每条都发**（用户视角的逐任务回执，不能省）；知会**每个项目只保留一条
//! 在途**：`has_unfinished_notice_to_agent` 命中就跳过，Owner 醒一次读到的已是项目
//! 最新状态。原 `TaskEventConsumer` 的项目级去重随职责迁移到本消费者，不要退回
//! 「每条完成都唤醒一次」——`Discard` 拦的是回复，不拦唤醒。
//!
//! # 为什么是 Sync
//!
//! 交付消息不能押在 Async 内存队列上：`aop::publish` fire-and-forget、吞错、队列纯
//! 内存、进程重启即丢，而任务状态落库已经是事实。`Registry::publish` 对 Sync 消费者
//! **内联执行** `on_event`，publish 返回时交付消息已落库 —— 「任务已 Completed 但
//! 用户永远收不到结果」这个窗口从根上不存在。
//!
//! ⚠️ 与 [`super::task_event_consumer::TaskEventConsumer`]（Async）订阅同一 topic，
//! 职责严格不重叠：那边只做 DAG 后继补发，不再发任何完成知会。改回 Async 必须
//! 同步补一条「任务行是权威待办」的补偿扫描，否则重新引入丢件窗口。
//!
//! # 两类字段来源不同
//!
//! - **身份**（`root_user_id` / `initiator_agent_id` / `trigger_message_id`）取事件
//!   **发布时快照**：Async 时间窗内任务/项目行可能已被改，反查会把结果送给错的人。
//! - **内容**（`execution_result` / `artifacts`）在这里按 `task_id` **现取**：
//!   `mark_done` 那一刻 Agent 可能还没写执行结果、还没落产物，随事件快照会拿到空值，
//!   交付消息就退化成一条空洞通知。Sync 内联恰好能读到「完成动作发生瞬间」的行状态。
//!
//! # 失败口径
//!
//! 沿用既有红线：状态写入已经是事实，不该因通知链路失败而回滚。所有投递失败只记
//! 日志并返回 `Ok(())`，不把状态事件打回重试（重试会重复投递）。
//! ⚠️ 若将来给 Sync 消费者加 Retry，必须先补「同 task_id + TaskCompleted 已投递」幂等。

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
use crate::service::domain::message::builder::{
    TaskCompletionView, build_task_completion_content, build_task_completion_notice,
};
use crate::service::domain::message::{SendToAgentCommand, SendToUserCommand};

pub struct TaskCompletionConsumer;

impl TaskCompletionConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskCompletionConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for TaskCompletionConsumer {
    fn name(&self) -> &str {
        "task_completion_delivery"
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::new(EventTopic::TaskStatusChanged)]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: TaskStatusChangedEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!(
                "failed to deserialize TaskStatusChangedEvent: {}",
                e
            ))
        })?;

        if event.new_status != TaskStatus::Completed {
            return Ok(());
        }

        // ① 项目行只查一次：owner（知会收件人）+ 旧事件缺快照时的归属用户回退。
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
                        "task_completion_delivery",
                        project_id = %pid,
                        error = ?e,
                        "查询项目失败，完成知会改用事件快照身份"
                    );
                    None
                }
            },
            None => None,
        };

        let root_user_id = if event.root_user_id.is_empty() {
            project
                .as_ref()
                .map(|p| p.po.root_user_id.as_str())
                .unwrap_or_default()
        } else {
            event.root_user_id.as_str()
        };

        let ctx = crate::consumer::enrich_org_from_project_user(&ctx, root_user_id).await;

        // ② 内容现取：执行结果 + 任务级产物清单（Domain 聚合，含软删除过滤）
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
                    "task_completion_delivery",
                    task_id = %event.task_id,
                    error = ?e,
                    "读取任务执行结果与产物失败，交付消息按空值降级"
                );
                None
            }
        };
        let execution_result = task.as_ref().and_then(|t| t.po.execution_result.as_deref());
        let artifacts: Vec<ArtifactDetail> = task
            .as_ref()
            .and_then(|t| t.artifacts.clone())
            .unwrap_or_default();

        let content = build_task_completion_content(&TaskCompletionView {
            task_id: &event.task_id,
            task_title: &event.task_title,
            project_id: event.project_id.as_deref(),
            assignee_id: &event.assignee_id,
            root_user_id,
            initiator_agent_id: &event.initiator_agent_id,
            status: event.new_status,
            progress: event.progress,
            execution_result,
            artifacts: &artifacts,
        });

        // ⚠️ 所有派生消息继承任务诞生时的锚点，保证「派单 → 交付 → 知会」在同一条链
        let reply_to_id =
            (!event.trigger_message_id.is_empty()).then_some(event.trigger_message_id.as_str());
        let message_domain = crate::service::domain::message::domain();

        // ③ 交付消息直达归属用户（用户视角一直和发起者对话，from 优先取发起者）
        let from_agent_id = if !event.initiator_agent_id.is_empty() {
            event.initiator_agent_id.as_str()
        } else if !event.assignee_id.is_empty() {
            event.assignee_id.as_str()
        } else {
            "system"
        };

        if root_user_id.is_empty() {
            log_warn!(
                &ctx,
                "task_completion_delivery",
                task_id = %event.task_id,
                "任务完成但无归属用户，跳过交付消息（A2A / 存量任务）"
            );
        } else if let Err(e) = message_domain
            .delivery()
            .send_to_user_with_type(
                ctx.clone(),
                SendToUserCommand {
                    from_agent_id,
                    to_user_id: root_user_id,
                    content: &content,
                    project_id: event.project_id.as_deref(),
                    task_id: Some(&event.task_id),
                    reply_to_id,
                },
                MessageType::TaskCompleted,
            )
            .await
        {
            log_warn!(
                &ctx,
                "task_completion_delivery",
                task_id = %event.task_id,
                to_user_id = %root_user_id,
                error = ?e,
                "任务完成交付消息投递失败"
            );
        }

        // ④ 知会只给「统筹方」，且不与交付消息重复内容。owner 未配置则退回任务
        // 发起者；发起者恰好就是执行 Agent 时不自己知会自己。
        let owner_agent_id = project
            .as_ref()
            .and_then(|p| p.po.owner_agent_id.as_deref())
            .filter(|s| !s.is_empty());
        let notify_agent_id = owner_agent_id.or_else(|| {
            (!event.initiator_agent_id.is_empty()).then_some(event.initiator_agent_id.as_str())
        });
        // ⚠️ 不做「自己知会自己」过滤：知会是 NotifyOnly + 静态 Discard，多一轮
        // 无副作用唤醒；而漏掉统筹方的完成信号会让项目编排停摆。这正是既有
        // 「宁可多跑一轮核对，也不能因『可能有人处理过』而不发」的取舍。

        if let Some(notify_agent_id) = notify_agent_id {
            // ⑤ 项目级合并：同一项目已经有**在途**的完成知会就不再发第二条。
            // 一个项目连续完成 5 个任务时，Owner 只需要一次「有交付了，去核对项目
            // 状态」的信号 —— 每条知会都会先唤醒一轮完整思考，`ReplyObligation` 的
            // NotifyOnly 只让 Final 被 Discard，省不掉唤醒成本。
            // 去重查询失败按「未重复」处理：漏发知会比多唤醒一轮更糟（编排可能停摆）。
            if let Some(project_id) = event.project_id.as_deref() {
                match message_domain
                    .has_unfinished_notice_to_agent(
                        ctx.clone(),
                        project_id,
                        notify_agent_id,
                        MessageType::TaskCompleted,
                    )
                    .await
                {
                    Ok(true) => {
                        log_debug!(
                            &ctx,
                            "task_completion_delivery",
                            task_id = %event.task_id,
                            agent_id = %notify_agent_id,
                            "同一项目已有在途完成知会，跳过本次知会（合并为一次核对）"
                        );
                        return Ok(());
                    }
                    Ok(false) => {}
                    Err(e) => {
                        log_warn!(
                            &ctx,
                            "task_completion_delivery",
                            task_id = %event.task_id,
                            agent_id = %notify_agent_id,
                            error = ?e,
                            "完成知会去重查询失败，按未重复处理（宁可多唤醒一轮）"
                        );
                    }
                }
            }

            let notice =
                build_task_completion_notice(&event.task_title, &event.task_id, artifacts.len());
            let cmd = SendToAgentCommand {
                from_id: from_agent_id,
                from_role: MessageRole::Agent,
                to_agent_id: notify_agent_id,
                content: &notice,
                project_id: event.project_id.as_deref(),
                task_id: Some(&event.task_id),
                reply_to_id,
                external: Default::default(),
                attachment_ids: None,
                // 完成知会与交付回执同属「任务完成」动作类型：`task_completed_notify`
                // 静态规则已在交付责任制两跳之前把它拦成 Discard，且 `ReplyObligation`
                // 对 TaskCompleted 返回 NotifyOnly —— 路由与提示词成对，不会乒乓。
                message_type: MessageType::TaskCompleted,
            };
            if let Err(e) = message_domain
                .delivery()
                .send_to_agent(ctx.clone(), cmd)
                .await
            {
                log_warn!(
                    &ctx,
                    "task_completion_delivery",
                    task_id = %event.task_id,
                    agent_id = %notify_agent_id,
                    error = ?e,
                    "任务完成知会投递失败"
                );
            }
        }

        Ok(())
    }
}
