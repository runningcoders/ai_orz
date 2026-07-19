//! Message Topic 消费者
//!
//! 负责消费所有类型的消息（用户消息、Agent 间消息、工具调用等）

use crate::service::dal::agent::AgentFetchOptions;
use super::{GenericConsumer, MessageFetcher, MessageHandler};
use common::error::{Error, Result};
use crate::models::message::{Message, ToolCallMessage};
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::service::domain::message::{
    DeliverMessageCommand, MessageDomain, SendToolCallResultCommand, ToolCallExecutionOutcome,
};
use crate::service::domain::runtime::RuntimeDomain;
use async_trait::async_trait;
use common::config::TopicConsumerConfig;
use common::enums::{MessageRole, MessageType};
use serde_json::Value;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

/// Message 消费者单例
static MESSAGE_CONSUMER: OnceLock<Arc<MessageConsumer>> = OnceLock::new();

// ==================== 类型定义 ====================

/// Message 拉取器：封装 domain 层调用
pub struct MessageFetcherImpl;

/// Message 处理器：业务逻辑
pub struct MessageHandlerImpl {
    runtime_domain: Arc<dyn RuntimeDomain>,
    message_domain: Arc<dyn MessageDomain>,
    hr_domain: Arc<dyn crate::service::domain::hr::HrDomain>,
    project_domain: Arc<dyn crate::service::domain::project::ProjectDomain>,
}

/// Message 消费者具体类型
pub type MessageConsumer = GenericConsumer<Message, MessageFetcherImpl, MessageHandlerImpl>;

// ==================== Fetcher 实现（调用 domain 层） ====================

#[async_trait]
impl MessageFetcher<Message> for MessageFetcherImpl {
    async fn dequeue_next(&self) -> Result<Option<Message>> {
        // 创建系统上下文
        let ctx = crate::pkg::RequestContext::new(None, None);
        // 调用 domain 层拉取消息
        crate::service::domain::message::domain()
            .delivery()
            .dequeue_next(ctx)
            .await
    }

    async fn ack(&self, event_id: &str) -> Result<()> {
        let ctx = crate::pkg::RequestContext::new(None, None);
        // 调用 domain 层确认消息
        crate::service::domain::message::domain()
            .delivery()
            .ack(ctx, event_id)
            .await
    }

    async fn nack(&self, event_id: &str) -> Result<()> {
        let ctx = crate::pkg::RequestContext::new(None, None);
        // 调用 domain 层标记失败
        crate::service::domain::message::domain()
            .delivery()
            .nack(ctx, event_id)
            .await
    }
}

// ==================== Handler 实现（业务逻辑） ====================\n

#[async_trait]
impl MessageHandler<Message> for MessageHandlerImpl {
    async fn handle(&self, message: &Message) -> Result<()> {
        sys_debug!(
            r"received message: {:?} -> {:?}, type: {:?}",
            message.from_role(),
            message.to_role(),
            message.message_type()
        );

        // 第一层分发：根据 to_role 决定谁来处理
        match message.to_role() {
            MessageRole::Agent => {
                // 发给 Agent → Brain 思考
                self.handle_agent_message(message).await?;
            }
            MessageRole::User => {
                // 发给用户 → 网关推送
                self.handle_user_message(message).await?;
            }
            MessageRole::System => {
                // 发给系统 → 工具执行或其他系统任务
                self.handle_system_message(message).await?;
            }
        }

        Ok(())
    }
}

// ==================== 各处理者逻辑 ====================\n

impl MessageHandlerImpl {
    /// 创建生产处理器（使用全局 Domain 单例）
    pub fn new() -> Self {
        Self {
            runtime_domain: crate::service::domain::runtime::domain(),
            message_domain: crate::service::domain::message::domain(),
            hr_domain: crate::service::domain::hr::domain(),
            project_domain: crate::service::domain::project::domain(),
        }
    }

    /// 创建测试处理器（显式注入 Domain，避免绑定全局单例）
    #[cfg(test)]
    pub fn new_for_test(
        runtime_domain: Arc<dyn RuntimeDomain>,
        message_domain: Arc<dyn MessageDomain>,
        hr_domain: Arc<dyn crate::service::domain::hr::HrDomain>,
        project_domain: Arc<dyn crate::service::domain::project::ProjectDomain>,
    ) -> Self {
        Self {
            runtime_domain,
            message_domain,
            hr_domain,
            project_domain,
        }
    }

    /// 从 MessagePo 重建 RequestContext
    fn rebuild_context(&self, message: &Message) -> crate::pkg::RequestContext {
        let mut builder = crate::pkg::RequestContext::builder();

        // organization_id
        if let Some(org_id) = &message.po.organization_id {
            builder = builder.organization_id(org_id.clone());
        }

        // user_id: from from_id based on from_role
        // If from_role is User, from_id is the user_id
        if message.from_role() == MessageRole::User {
            builder = builder.user_id(message.po.from_id.clone());
        }

        // project_id
        if let Some(project_id) = &message.po.project_id {
            builder = builder.project_id(project_id.clone());
        }

        // task_id
        if let Some(task_id) = &message.po.task_id {
            builder = builder.task_id(task_id.clone());
        }

        // agent_id: the receiving agent
        builder = builder.agent_id(message.po.to_id.clone());

        builder.build()
    }

    /// Agent 消息处理：调用 Brain 思考
    async fn handle_agent_message(&self, message: &Message) -> Result<()> {
        let agent_id = &message.po.to_id;

        // 消费前检查 Agent 是否可用（空闲）
        // 如果 Agent 忙碌或休息，返回错误触发 Nack，消息重新入队等待
        if AgentRuntimeStateManager::global().is_unavailable(agent_id) {
            return Err(Error::conflict(format!(
                "Agent {} is busy or resting, message will be retried",
                agent_id
            )));
        }

        // 重建上下文
        let ctx = self.rebuild_context(message);

        // 加载 Agent 实体（包含统计信息）
        let fetch_options = AgentFetchOptions {
            with_stats: Some(message.po.task_id.is_some()),
            stats_task_id: message.po.task_id.clone(),
            ..Default::default()
        };
        let mut agent = self
            .hr_domain
            .agent_manage()
            .get_agent(ctx.clone(), agent_id, fetch_options)
            .await?
            .ok_or_else(|| Error::not_found(format!("Agent {} not found", agent_id)))?;

        // 检查轮次限制：如果有 task_id，检查是否超过最大思考深度
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

                    // 发送提示消息给用户
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

                    return Ok(());
                }
            }
        }

        // 检查任务完成状态：如果任务已完成，不再唤醒 Agent
        if let Some(task_id) = &message.po.task_id {
            if let Ok(Some(task)) = self.project_domain.task_manage().get(ctx.clone(), task_id).await {
                if matches!(task.po.status, common::enums::TaskStatus::Completed | common::enums::TaskStatus::Cancelled | common::enums::TaskStatus::Archived) {
                    log_info!(
                        &ctx,
                        "handle_agent_message",
                        "Task {} is in {:?} state, skipping agent wake",
                        task_id,
                        task.po.status
                    );
                    return Ok(());
                }
            }
        }

        // 确保 Agent 有 Brain（已唤醒）
        // 如果 brain 未装配，通过 RuntimeDomain 自动装配
        if agent.brain.is_none() {
            log_info!(
                &ctx,
                "handle_agent_message",
                "Agent {} brain not initialized, auto waking brain",
                agent_id
            );
            self.runtime_domain
                .awakening()
                .wake_agent_brain(ctx.clone(), &mut agent)
                .await?;
        }

        // 调用 Runtime Domain 唤醒 Agent
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

    /// 用户消息处理：通过消息网关推送给前端
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
        Ok(())
    }

    /// 系统消息处理：执行工具或其他系统任务
    async fn handle_system_message(&self, message: &Message) -> Result<()> {
        match message.message_type() {
            MessageType::ToolCallRequest => self.handle_tool_call_request(message).await,
            _ => {
                sys_debug!("system message processed by system module");
                Ok(())
            }
        }
    }

    /// ToolCallRequest 处理：编排 Runtime Domain 执行工具，并通过 Message Domain 回写结果
    async fn handle_tool_call_request(&self, message: &Message) -> Result<()> {
        let tool_call = parse_tool_call_request(message)?;
        let args = tool_call.args.unwrap_or(Value::Null);

        let mut builder = crate::pkg::RequestContext::builder();
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
        if message.from_role() == MessageRole::User {
            builder = builder.user_id(message.po.from_id.clone());
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
            },
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
}

fn parse_tool_call_request(message: &Message) -> Result<ToolCallMessage> {
    if message.message_type() != MessageType::ToolCallRequest {
        return Err(common::error::Error::bad_request(format!(
            "expected ToolCallRequest message, got {:?}",
            message.message_type()
        )));
    }

    serde_json::from_str(&message.po.content)
        .map_err(|err| common::error::Error::bad_request(format!("invalid ToolCallRequest content: {}", err)))
}

fn tool_error_message(err: &Error) -> String {
    // err.msg is already the pure message without [code] prefix
    // No need to strip prefix because we avoid duplicate error wrapping at the source
    err.msg.clone()
}

// ==================== 初始化与单例访问 ====================

/// 获取 Message 消费者单例
///
/// 用于监控、统计、状态检查等场景
pub fn get_consumer() -> Option<&'static MessageConsumer> {
    MESSAGE_CONSUMER.get().map(|arc| &**arc)
}

/// 初始化并启动 Message 消费者
///
/// 由 consumer::init 调用
pub async fn init(config: &TopicConsumerConfig) -> Result<()> {
    sys_info!("initializing message consumer...");

    // 创建 fetcher 和 handler
    let fetcher = MessageFetcherImpl;
    let handler = MessageHandlerImpl::new();

    // 调用泛型 new 方法
    let consumer = MessageConsumer::new("message", config.clone(), fetcher, handler);

    // 设置单例
    let consumer_arc = Arc::new(consumer);
    MESSAGE_CONSUMER.set(consumer_arc.clone()).map_err(|_| {
        common::error::Error::internal("message consumer already initialized".to_string())
    })?;

    // 启动消费者
    consumer_arc.start().await;

    sys_info!("message consumer started");
    Ok(())
}

// ==================== 测试用方法 ====================

#[cfg(test)]
pub fn new_for_test<F, H>(
    config: TopicConsumerConfig,
    fetcher: F,
    handler: H,
) -> GenericConsumer<Message, F, H>
where
    F: MessageFetcher<Message> + Send + Sync + 'static,
    H: MessageHandler<Message> + Send + Sync + 'static,
{
    GenericConsumer::new("test", config, fetcher, handler)
}
