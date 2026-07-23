//! 消息消费者（业务层）
//!
//! 作为 AOP 事件中心的订阅者，消费 MESSAGE_CREATED 事件。
//! 本模块只负责"订阅 + 调度"，业务逻辑通过调用 domain 层完成：
//! - Agent 消息 → RuntimeDomain.awaken()
//! - User 消息 → MessageDomain.deliver_message()
//! - System 消息 → RuntimeDomain.tool_execution()
//!
//! 与 AOP 框架解耦：AOP 只负责事件流转，本模块负责业务编排。

use async_trait::async_trait;
use common::enums::{MessageRole, MessageStatus, MessageType};
use common::error::{Error, Result};
use serde_json::Value;
use std::sync::Arc;

use crate::models::message::{Message, ToolCallMessage};
use crate::models::events::MessageCreatedEvent;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::dal::message as message_dal;
use crate::service::domain::hr::{self as hr_domain, HrDomain};
use crate::service::domain::message::{
    self as message_domain, DeliverMessageCommand, MessageDomain, SendToolCallResultCommand,
    ToolCallExecutionOutcome,
};
use crate::service::domain::project::{self as project_domain, ProjectDomain};
use crate::service::domain::runtime::{self as runtime_domain, RuntimeDomain};

// ==================== 消费者实现 ====================

/// Agent 唤醒消费者
///
/// 订阅 MESSAGE_CREATED 事件，按 to_role 分发到不同 domain 处理。
/// 作为 AOP 的 Async 消费者，由 Registry 调度器自动轮询拉取。
pub struct MessageConsumer {
    runtime_domain: Arc<dyn RuntimeDomain>,
    message_domain: Arc<dyn MessageDomain>,
    hr_domain: Arc<dyn HrDomain>,
    project_domain: Arc<dyn ProjectDomain>,
}

impl MessageConsumer {
    pub fn new() -> Self {
        Self {
            runtime_domain: runtime_domain::domain(),
            message_domain: message_domain::domain(),
            hr_domain: hr_domain::domain(),
            project_domain: project_domain::domain(),
        }
    }
}

#[async_trait]
impl Consumer for MessageConsumer {
    fn name(&self) -> &str {
        "agent.awakening"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("message.created")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let msg_event: MessageCreatedEvent = serde_json::from_value(event)?;

        // 从 DB 加载完整 Message
        let ctx = RequestContext::new(None, None);
        let message = message_dal::dal()
            .find_by_id(ctx.clone(), &msg_event.message_id)
            .await?
            .ok_or_else(|| {
                Error::not_found(format!(
                    "Message {} not found",
                    msg_event.message_id
                ))
            })?;

        sys_debug!(
            "received message: {:?} -> {:?}, type: {:?}",
            message.from_role(),
            message.to_role(),
            message.message_type()
        );

        // 根据 to_role 分发到对应 domain
        match message.to_role() {
            MessageRole::Agent => {
                self.handle_agent_message(&message).await?;
            }
            MessageRole::User => {
                self.handle_user_message(&message).await?;
            }
            MessageRole::System => {
                self.handle_system_message(&message).await?;
            }
        }

        Ok(())
    }

    async fn ack(&self, event_id: &str) -> Result<()> {
        let ctx = RequestContext::new(None, None);
        message_dal::dal()
            .update_status(ctx, event_id, MessageStatus::Processed)
            .await?;
        Ok(())
    }

    async fn nack(&self, event_id: &str) -> Result<()> {
        let ctx = RequestContext::new(None, None);
        message_dal::dal()
            .update_status(ctx, event_id, MessageStatus::Pending)
            .await?;
        Ok(())
    }

    fn concurrency(&self) -> usize {
        4
    }

    fn empty_queue_sleep_ms(&self) -> u64 {
        100
    }

    fn error_retry_sleep_ms(&self) -> u64 {
        1000
    }
}

// ==================== 业务编排（调用 domain 层）====================

impl MessageConsumer {
    /// Agent 消息处理：调用 RuntimeDomain 唤醒 Agent
    async fn handle_agent_message(&self, message: &Message) -> Result<()> {
        let agent_id = &message.po.to_id;

        // 原子地占用 Agent（修复 TOCTOU 竞态）
        // 之前 is_unavailable + 后续 awaken 的 set_busy 之间存在窗口，4 个 worker 并发时
        // 同一 agent 收不同 project 消息会被两个 worker 同时通过检查
        let acquired = AgentRuntimeStateManager::global()
            .try_set_busy(agent_id, &message.po.id);
        if !acquired {
            return Err(Error::conflict(format!(
                "Agent {} is busy or resting, message will be retried",
                agent_id
            )));
        }
        // 注意：此时已 set_busy，后续失败路径必须 set_idle
        // awaken 内部会创建 BusyGuard 确保清理
        // 但 awaken 之前的失败（如 get_agent）需要显式清理

        let mut ctx = self.rebuild_context(message);

        // 加载 Agent 实体（包含工具 + 技能 + 统计信息，供唤醒流程使用）
        let fetch_options = AgentFetchOptions {
            with_tools: Some(true),
            with_skills: Some(true),
            with_stats: Some(message.po.task_id.is_some()),
            stats_task_id: message.po.task_id.clone(),
            ..Default::default()
        };
        let agent_result = self
            .hr_domain
            .agent_manage()
            .get_agent(ctx.clone(), agent_id, fetch_options)
            .await;

        let mut agent = match agent_result {
            Ok(Some(a)) => a,
            Ok(None) => {
                // Agent 不存在：永久错误，不应无限重试
                // 释放 Busy 状态并返回非重试错误
                AgentRuntimeStateManager::global().set_idle(agent_id);
                return Err(Error::not_found(format!(
                    "Agent {} not found, message will not be retried",
                    agent_id
                )));
            }
            Err(e) => {
                // 查询失败：临时错误，释放 Busy 允许重试
                AgentRuntimeStateManager::global().set_idle(agent_id);
                return Err(e);
            }
        };

        // 检查任务完成状态（优先于 thinking_depth 检查）
        // 顺序说明：若任务已 Completed/Cancelled，应直接跳过唤醒，避免向已结束的任务
        // 发送误导性的"达到最大思考深度"消息
        if let Some(task_id) = &message.po.task_id {
            match self.project_domain.task_manage().get(ctx.clone(), task_id).await {
                Ok(Some(task)) => {
                    if matches!(task.po.status, common::enums::TaskStatus::Completed | common::enums::TaskStatus::Cancelled | common::enums::TaskStatus::Archived) {
                        log_info!(
                            &ctx,
                            "handle_agent_message",
                            "Task {} is in {:?} state, skipping agent wake",
                            task_id,
                            task.po.status
                        );
                        // 释放 Busy 状态（awaken 不会被调用）
                        AgentRuntimeStateManager::global().set_idle(agent_id);
                        return Ok(());
                    }
                }
                Ok(None) => {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "task {} not found, skip status check",
                        task_id
                    );
                }
                Err(e) => {
                    // 查询失败：临时错误，释放 Busy 允许重试
                    AgentRuntimeStateManager::global().set_idle(agent_id);
                    return Err(e);
                }
            }
        }

        // 检查轮次限制
        if let (Some(_task_id), Some(stats)) = (&message.po.task_id, &agent.stats) {
            if let Some(call_summary) = &stats.call_summary {
                let runtime_config = agent.po.get_runtime_config();
                let max_depth = runtime_config.max_thinking_depth as u64;
                if call_summary.total_calls >= max_depth {
                    log_warn!(
                        &ctx,
                        "handle_agent_message",
                        "Agent {} reached max thinking depth ({}), stopping loop",
                        agent_id,
                        max_depth
                    );

                    let _ = self.message_domain
                        .delivery()
                        .send_to_user(
                            ctx.clone(),
                            crate::service::domain::message::SendToUserCommand {
                                from_agent_id: agent_id,
                                to_user_id: &message.po.from_id,
                                content: &format!(
                                    "Agent has reached the maximum thinking depth ({} turns). The task has been stopped to prevent infinite loops.",
                                    max_depth
                                ),
                                project_id: message.po.project_id.as_deref(),
                                task_id: message.po.task_id.as_deref(),
                                reply_to_id: None,
                            },
                        )
                        .await;

                    // 释放 Busy 状态（awaken 不会被调用，BusyGuard 不会创建）
                    AgentRuntimeStateManager::global().set_idle(agent_id);
                    return Ok(());
                }
            }
        }

        // 确保 Agent 有 Brain
        // wake_agent_brain 内部会查询 ModelProvider 并 enrich ctx
        // （补充 model_provider_id / model_name 字段），返回的新 ctx 用于后续 awaken
        if agent.brain.is_none() {
            log_info!(
                &ctx,
                "handle_agent_message",
                "Agent {} brain not initialized, auto waking brain",
                agent_id
            );
            let enriched_ctx = self
                .runtime_domain
                .awakening()
                .wake_agent_brain(ctx, &mut agent)
                .await
                .map_err(|e| {
                    // wake_agent_brain 失败：释放 Busy 允许重试
                    // （awaken 未被调用，BusyGuard 未创建）
                    AgentRuntimeStateManager::global().set_idle(agent_id);
                    e
                })?;
            ctx = enriched_ctx;
        }

        // 调用 RuntimeDomain 唤醒 Agent
        let awaken_result = self
            .runtime_domain
            .awakening()
            .awaken(ctx.clone(), &agent, message)
            .await?;

        log_info!(
            &ctx,
            "handle_agent_message",
            "Agent {} awakened successfully, trace_ids: {:?}",
            agent_id,
            awaken_result.trace_ids
        );

        Ok(())
    }

    /// User 消息处理：调用 MessageDomain 推送给用户
    async fn handle_user_message(&self, message: &Message) -> Result<()> {
        let ctx = self.rebuild_context(message);
        let cmd = DeliverMessageCommand {
            message,
            user_id: &message.po.to_id,
        };
        let result = self.message_domain
            .delivery()
            .deliver_message(ctx, cmd)
            .await?;

        sys_debug!(
            "user message delivered: sse={}, channels={}/{}",
            result.sse_delivered,
            result.success,
            result.total
        );

        // 修复：所有渠道投递失败时返回错误，触发 nack 重试
        // 之前即使 success=0 也返回 Ok(())，消息被 ack 标记为 Processed，永远不会重试
        if result.success == 0 && result.sse_delivered == 0 {
            return Err(Error::internal(format!(
                "All delivery channels failed for message {}, will retry",
                message.po.id
            )));
        }

        Ok(())
    }

    /// System 消息处理：按类型分发
    async fn handle_system_message(&self, message: &Message) -> Result<()> {
        match message.message_type() {
            MessageType::ToolCallRequest => self.handle_tool_call_request(message).await,
            _ => {
                sys_debug!("system message processed by system module");
                Ok(())
            }
        }
    }

    /// ToolCallRequest 处理：调用 RuntimeDomain 执行工具，MessageDomain 回写结果
    async fn handle_tool_call_request(&self, message: &Message) -> Result<()> {
        let tool_call = parse_tool_call_request(message)?;
        let args = tool_call.args.unwrap_or(Value::Null);

        let mut builder = RequestContext::builder();
        builder = builder.agent_id(tool_call.from_id.clone());
        if let Some(project_id) = &tool_call.project_id {
            builder = builder.project_id(project_id.clone());
        }
        if let Some(task_id) = &tool_call.task_id {
            builder = builder.task_id(task_id.clone());
        }
        if let Some(org_id) = &message.po.organization_id {
            builder = builder.organization_id(org_id.clone());
        }
        // 修复：从 ToolCallMessage 回填 ctx 字段，与同步路径保持一致
        // 之前 from_role=Agent 时 user_id 永远不会被设置，
        // log_id 重新生成与触发轮次断链，model_provider_id / model_name 全部丢失
        if let Some(log_id) = &tool_call.from_log_id {
            builder = builder.log_id(log_id.clone());
        }
        if let Some(user_id) = &tool_call.from_user_id {
            builder = builder.user_id(user_id.clone());
        } else if message.from_role() == MessageRole::User {
            builder = builder.user_id(message.po.from_id.clone());
        }
        if let Some(model_provider_id) = &tool_call.from_model_provider_id {
            builder = builder.model_provider_id(model_provider_id.clone());
        }
        if let Some(model_name) = &tool_call.from_model_name {
            builder = builder.model_name(model_name.clone());
        }
        let ctx = builder.build();

        let execution = self
            .runtime_domain
            .tool_execution()
            .call_manual_tool_for_agent(
                ctx.clone(),
                tool_call.from_id.clone(),
                tool_call.tool_id.clone(),
                args,
            )
            .await;

        let outcome = match execution {
            Ok(execution_result) => ToolCallExecutionOutcome::Success {
                result: execution_result.result,
                result_file_meta: None,
                trace_ref: Some(execution_result.trace_ref),
            },
            Err(err) => {
                let trace_ref = err.field().and_then(|f| f.trace_ref.clone());
                ToolCallExecutionOutcome::Failure {
                    error_message: tool_error_message(&err),
                    trace_ref,
                }
            }
        };

        self.message_domain
            .delivery()
            .send_tool_call_result(
                ctx,
                SendToolCallResultCommand {
                    request_message: message,
                    outcome,
                },
            )
            .await?;

        Ok(())
    }

    /// 从 MessagePo 重建 RequestContext
    fn rebuild_context(&self, message: &Message) -> RequestContext {
        let mut builder = RequestContext::builder();

        if let Some(org_id) = &message.po.organization_id {
            builder = builder.organization_id(org_id.clone());
        }

        if message.from_role() == MessageRole::User {
            builder = builder.user_id(message.po.from_id.clone());
        }

        if let Some(project_id) = &message.po.project_id {
            builder = builder.project_id(project_id.clone());
        }

        if let Some(task_id) = &message.po.task_id {
            builder = builder.task_id(task_id.clone());
        }

        builder = builder.agent_id(message.po.to_id.clone());

        builder.build()
    }
}

// ==================== 辅助函数 ====================

fn parse_tool_call_request(message: &Message) -> Result<ToolCallMessage> {
    if message.message_type() != MessageType::ToolCallRequest {
        return Err(Error::bad_request(format!(
            "expected ToolCallRequest message, got {:?}",
            message.message_type()
        )));
    }

    serde_json::from_str(&message.po.content)
        .map_err(|err| Error::bad_request(format!("invalid ToolCallRequest content: {}", err)))
}

fn tool_error_message(err: &Error) -> String {
    err.msg.clone()
}
