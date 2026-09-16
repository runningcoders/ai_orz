//! A2A 远端任务轮询消费者
//!
//! 订阅 [`EventTopic::A2aPollRequested`]（生产者每 30s 对每个远端 Agent 发一条认领事件）：
//! 拉取该 Agent 名下全部 InProgress 任务 → 把远端新增的 agent 消息投递给任务归属用户
//! → 推进 task tags 里的同步计数与本地任务状态。
//!
//! **为什么把这段从生产者搬出来**：原 `A2aPollingProducer::poll` 在轮询线程里直接做
//! 远端 HTTP 拉取 + 消息投递，慢业务会把整个轮询周期拖长；且它**不经过事件中心**，
//! 既无归属也无回调。搬进消费者后落回统一模型：生产者只「认领」，执行在 Async worker。
//!
//! **Async + ordered**：`order_key = agent_id`，同一 Agent 的相邻两轮落在同一队列串行
//! —— 这正是生产者可以「只认领、不等结果」的前提（上一轮未完成时下一轮排队等待）。

use async_trait::async_trait;
use common::enums::{AssigneeType, CallerType, EventTopic, TaskStatus};
use common::error::{Error, Result};

use crate::models::events::{
    A2A_SYNCED_MSG_COUNT_PREFIX, A2aPollRequestedEvent, extract_a2a_task_id,
    extract_text_from_parts, get_synced_msg_count, make_synced_msg_tag,
};
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::domain::hr as hr_domain;
use crate::service::domain::message::{self as message_domain, SendToUserCommand};
use crate::service::domain::project as project_domain;

// ==================== 消费者实现 ====================

/// A2A 远端任务轮询消费者
pub struct A2aPollConsumer;

impl Default for A2aPollConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl A2aPollConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for A2aPollConsumer {
    fn name(&self) -> &str {
        "a2a_poll"
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        // `.ordered()`：同一 Agent 的相邻两轮必须串行 —— 生产者「只认领」的前提。
        // 本消费者是 Async（默认并发 1），`ordered` 目前只在并发 > 1 时可观测；
        // 显式声明是为了把契约钉死，避免将来调高并发时静默失去串行保证。
        vec![Subscription::new(EventTopic::A2aPollRequested).ordered()]
    }

    fn consume_mode(&self) -> ConsumeMode {
        // 远端 HTTP 拉取 + 消息投递都可能在网络/DB 上耗时：绝不能在生产者轮询线程里做
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: A2aPollRequestedEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!(
                "failed to deserialize A2aPollRequestedEvent: {}",
                e
            ))
        })?;
        poll_agent(ctx, &event.agent_id).await
    }
}

// ==================== 轮询处理（原生产者 tick 的内层逻辑）====================

/// 处理一个远端 Agent 的一轮轮询
///
/// 失败语义：本消费者对**单点失败降级为 warn + skip**（远端拉取失败 / 消息投递失败 /
/// 状态推进失败都不上抛）—— 下一轮 tick 会重新认领同一 Agent 重来，比 nack 重投更快
/// 收敛（且远端网络抖动不值得走无限重投）。真正的「整轮失败」（如查询本地任务列表
/// 报错）才上抛 `Err`。
async fn poll_agent(ctx: RequestContext, agent_id: &str) -> Result<()> {
    let Some(agent) = hr_domain::domain()
        .agent_manage()
        .get_agent(ctx.clone(), agent_id, AgentFetchOptions::default())
        .await?
    else {
        log_warn!(&ctx, "a2a_poll", "agent {} not found (skip)", agent_id);
        return Ok(());
    };

    // 认领时是远端、执行时可能已改类型（如已转本地）→ 再判一次，避免无谓拉取
    if !agent.po.kind.is_remote() {
        log_debug!(
            &ctx,
            "a2a_poll",
            "agent {} is no longer remote (skip)",
            agent_id
        );
        return Ok(());
    }

    let tasks = project_domain::domain()
        .task_manage()
        .list(
            ctx.clone(),
            None,
            Some(AssigneeType::Agent),
            Some(&agent.po.id),
            Some(TaskStatus::InProgress),
            Some(100),
        )
        .await?;

    if tasks.is_empty() {
        return Ok(());
    }

    let mut processed_count = 0usize;

    for task in &tasks {
        let tags = task.po.get_tags();
        let Some(remote_task_id) = extract_a2a_task_id(&tags) else {
            continue;
        };

        // 远端任务拉取走 hr domain（运行时配置解析在 DAL 内完成），
        // 配置缺失/非法与网络失败统一在此降级为 warn + skip
        let remote_task = match hr_domain::domain()
            .agent_manage()
            .fetch_remote_task(ctx.clone(), &agent, &remote_task_id)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                log_warn!(
                    &ctx,
                    "a2a_poll",
                    "Failed to fetch remote task {} for local task {} (agent {}): {}",
                    remote_task_id,
                    task.po.id,
                    agent.po.id,
                    e
                );
                continue;
            }
        };

        // 每任务独立 ctx（System / agent / task / project）——与搬迁前逐字一致
        let mut task_ctx_builder = RequestContext::builder()
            .caller_type(CallerType::System)
            .agent_id(agent.po.id.clone())
            .task_id(task.po.id.clone());
        if let Some(pid) = &task.po.project_id {
            task_ctx_builder = task_ctx_builder.project_id(pid.clone());
        }
        let task_ctx = task_ctx_builder.build();

        let already_synced = get_synced_msg_count(&tags);
        let agent_messages: Vec<_> = remote_task
            .messages
            .iter()
            .filter(|msg| msg.role == "agent" || msg.role == "assistant")
            .collect();
        let total_agent_msgs = agent_messages.len();
        let mut new_sent = 0usize;

        if total_agent_msgs > already_synced {
            let new_messages = &agent_messages[already_synced..];
            for msg in new_messages {
                let text = extract_text_from_parts(&msg.parts);
                if text.is_empty() {
                    continue;
                }

                let cmd = SendToUserCommand {
                    from_agent_id: &agent.po.id,
                    to_user_id: &task.po.root_user_id,
                    content: &text,
                    project_id: task.po.project_id.as_deref(),
                    task_id: Some(&task.po.id),
                    reply_to_id: None,
                };

                if let Err(e) = message_domain::domain()
                    .delivery()
                    .send_to_user(task_ctx.clone(), cmd)
                    .await
                {
                    log_warn!(
                        &task_ctx,
                        "a2a_poll",
                        "Failed to send message for task {}: {}",
                        task.po.id,
                        e
                    );
                } else {
                    new_sent += 1;
                }
            }
        }

        if new_sent > 0 {
            let new_total = already_synced + new_sent;
            let mut new_tags: Vec<String> = tags
                .iter()
                .filter(|t| !t.starts_with(A2A_SYNCED_MSG_COUNT_PREFIX))
                .cloned()
                .collect();
            new_tags.push(make_synced_msg_tag(new_total));

            if let Err(e) = project_domain::domain()
                .task_manage()
                .update_basic(
                    task_ctx.clone(),
                    &task.po.id,
                    None,
                    None,
                    None,
                    Some(new_tags),
                    None,
                    None,
                    None,
                    None,
                )
                .await
            {
                log_warn!(
                    &task_ctx,
                    "a2a_poll",
                    "Failed to update synced msg count for task {}: {}",
                    task.po.id,
                    e
                );
            }
        }

        let mut local_task = task.clone();
        let target_status = match remote_task.status.state {
            common::api::a2a::A2aTaskState::Completed => Some(TaskStatus::Completed),
            common::api::a2a::A2aTaskState::Failed => Some(TaskStatus::Cancelled),
            common::api::a2a::A2aTaskState::Canceled => Some(TaskStatus::Cancelled),
            common::api::a2a::A2aTaskState::Working
            | common::api::a2a::A2aTaskState::Submitted
            | common::api::a2a::A2aTaskState::InputRequired => {
                if local_task.po.status == TaskStatus::Pending {
                    Some(TaskStatus::InProgress)
                } else {
                    None
                }
            }
        };

        if let Some(target) = target_status
            && local_task.po.status != target
        {
            if let Err(e) = project_domain::domain()
                .task_manage()
                .transition_status(task_ctx.clone(), &mut local_task, target)
                .await
            {
                log_warn!(
                    &task_ctx,
                    "a2a_poll",
                    "Failed to transition task {} to {:?}: {}",
                    task.po.id,
                    target,
                    e
                );
            } else {
                log_info!(
                    &task_ctx,
                    "a2a_poll",
                    "Task {} transitioned to {:?}",
                    task.po.id,
                    target
                );
            }
        }

        processed_count += 1;
    }

    if processed_count > 0 {
        log_info!(
            &ctx,
            "a2a_poll",
            "agent {} processed {} tasks",
            agent_id,
            processed_count
        );
    }

    Ok(())
}
