//! Message Topic 消费者
//!
//! 负责消费所有类型的消息（用户消息、Agent 间消息、工具调用等）

use super::{GenericConsumer, MessageFetcher, MessageHandler};
use async_trait::async_trait;
use common::config::TopicConsumerConfig;
use common::enums::{MessageRole, MessageType};
use crate::error::Result;
use crate::models::message::Message;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

/// Message 消费者单例
static MESSAGE_CONSUMER: OnceLock<Arc<MessageConsumer>> = OnceLock::new();

// ==================== 类型定义 ====================

/// Message 拉取器：封装 domain 层调用
pub struct MessageFetcherImpl;

/// Message 处理器：业务逻辑
pub struct MessageHandlerImpl;

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
        tracing::debug!("received message: {:?} -> {:?}, type: {:?}", 
            message.from_role(), message.to_role(), message.message_type());

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
    /// Agent 消息处理：调用 Brain 思考
    async fn handle_agent_message(&self, _message: &Message) -> Result<()> {
        // TODO: 调用 BrainDomain.process_message
        // 1. 获取 Agent 上下文
        // 2. Brain 思考、生成回复
        // 3. 可能生成新的工具调用（to_role = System）
        tracing::debug!("agent message processed by brain");
        Ok(())
    }

    /// 用户消息处理：通过消息网关推送给前端
    async fn handle_user_message(&self, _message: &Message) -> Result<()> {
        // TODO: 调用 MessageGateway.push_to_user
        // 1. SSE/WebSocket 推送
        // 2. 在线状态检查
        tracing::debug!("user message pushed to frontend");
        Ok(())
    }

    /// 系统消息处理：执行工具或其他系统任务
    async fn handle_system_message(&self, _message: &Message) -> Result<()> {
        // TODO: 根据 message_type 分发到具体系统模块
        // ToolCallRequest → ToolDomain.execute_tool_call
        // 其他系统消息 → 对应处理
        tracing::debug!("system message processed by system module");
        Ok(())
    }
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
    tracing::info!("initializing message consumer...");

    // 创建 fetcher 和 handler
    let fetcher = MessageFetcherImpl;
    let handler = MessageHandlerImpl;

    // 调用泛型 new 方法
    let consumer = MessageConsumer::new("message", config.clone(), fetcher, handler);

    // 设置单例
    let consumer_arc = Arc::new(consumer);
    MESSAGE_CONSUMER
        .set(consumer_arc.clone())
        .map_err(|_| crate::error::AppError::Internal("message consumer already initialized".to_string()))?;

    // 启动消费者
    consumer_arc.start().await;

    tracing::info!("message consumer started");
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
