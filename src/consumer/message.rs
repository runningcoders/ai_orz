//! Message Topic 消费者
//!
//! 负责消费所有类型的消息（用户消息、Agent 间消息、工具调用等）

use super::{GenericConsumer, MessageFetcher, MessageHandler};
use crate::error::{AppError, Result};
use crate::models::message::{Message, ToolCallMessage};
use crate::service::domain::message::{
    MessageDomain, SendToolCallResultCommand, ToolCallExecutionOutcome,
};
use crate::service::domain::runtime::RuntimeDomain;
use async_trait::async_trait;
use common::config::TopicConsumerConfig;
use common::enums::{MessageRole, MessageType};
use rig::tool::ToolError;
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
        }
    }

    /// 创建测试处理器（显式注入 Domain，避免绑定全局单例）
    #[cfg(test)]
    pub fn new_for_test(
        runtime_domain: Arc<dyn RuntimeDomain>,
        message_domain: Arc<dyn MessageDomain>,
    ) -> Self {
        Self {
            runtime_domain,
            message_domain,
        }
    }

    /// Agent 消息处理：调用 Brain 思考
    async fn handle_agent_message(&self, _message: &Message) -> Result<()> {
        // TODO: 调用 BrainDomain.process_message
        // 1. 获取 Agent 上下文
        // 2. Brain 思考、生成回复
        // 3. 可能生成新的工具调用（to_role = System）
        sys_debug!("agent message processed by brain");
        Ok(())
    }

    /// 用户消息处理：通过消息网关推送给前端
    async fn handle_user_message(&self, _message: &Message) -> Result<()> {
        // TODO: 调用 MessageGateway.push_to_user
        // 1. SSE/WebSocket 推送
        // 2. 在线状态检查
        sys_debug!("user message pushed to frontend");
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

        let mut ctx = crate::pkg::RequestContext::new(None, None);
        ctx.set_agent_id(tool_call.from_id.clone());
        if let Some(project_id) = &tool_call.project_id {
            ctx.set_project_id(project_id.clone());
        }
        if let Some(task_id) = &tool_call.task_id {
            ctx.set_task_id(task_id.clone());
        }

        let outcome = match self
            .runtime_domain
            .tool_execution()
            .call_manual_tool_for_agent(
                ctx.clone(),
                tool_call.from_id.clone(),
                tool_call.tool_id.clone(),
                args,
            )
            .await
        {
            Ok(result) => ToolCallExecutionOutcome::Success {
                result,
                result_file_meta: None,
            },
            Err(err) => ToolCallExecutionOutcome::Failure {
                error_message: tool_error_message(err),
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
        return Err(AppError::BadRequest(format!(
            "expected ToolCallRequest message, got {:?}",
            message.message_type()
        )));
    }

    serde_json::from_str(&message.po.content)
        .map_err(|err| AppError::BadRequest(format!("invalid ToolCallRequest content: {}", err)))
}

fn tool_error_message(err: ToolError) -> String {
    let message = err.to_string();
    message
        .strip_prefix("ToolCallError: ")
        .unwrap_or(&message)
        .to_string()
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
        crate::error::AppError::Internal("message consumer already initialized".to_string())
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
