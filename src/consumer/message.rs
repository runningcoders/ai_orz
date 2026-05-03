//! Message Topic 消费者
//!
//! 负责消费所有类型的消息（用户消息、Agent 间消息、工具调用等）

use super::{GenericConsumer, MessageFetcher, MessageHandler};
use async_trait::async_trait;
use common::config::TopicConsumerConfig;
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

// ==================== Handler 实现（业务逻辑） ====================

#[async_trait]
impl MessageHandler<Message> for MessageHandlerImpl {
    async fn handle(&self, message: &Message) -> Result<()> {
        // TODO: 按 message_type 路由到对应 domain 业务方法
        // 目前先空实现，后续逐步完善

        tracing::debug!("received message: {:?}", message.message_type());

        // 暂时直接返回成功
        Ok(())
    }
}

// ==================== 初始化方法 ====================

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
