//! 消费者模块
//!
//! 与 handlers 同级，负责从队列消费消息并执行业务逻辑。
//! 调用链路: consumer → domain → dal → dao

use async_trait::async_trait;
use common::config::TopicConsumerConfig;
use crate::error::Result;
use crate::models::event::Event;
use std::marker::PhantomData;
use std::sync::Arc;

pub mod message;

// ==================== Trait 定义 ====================

/// 消息拉取 trait
///
/// 负责从队列拉取消息、确认、重试。
/// 由具体消费者实现，内部调用 domain 层方法。
#[async_trait]
pub trait MessageFetcher<E: Event> {
    /// 拉取下一条待消费的消息
    async fn dequeue_next(&self) -> Result<Option<E>>;

    /// 确认消息消费成功
    async fn ack(&self, event_id: &str) -> Result<()>;

    /// 标记消息消费失败，等待重试
    async fn nack(&self, event_id: &str) -> Result<()>;
}

/// 消息处理 trait
///
/// 负责具体业务逻辑处理。
/// 由具体消费者实现，内部调用各 domain 业务方法。
#[async_trait]
pub trait MessageHandler<E: Event> {
    /// 处理消息
    async fn handle(&self, event: &E) -> Result<()>;
}

// ==================== 通用泛型消费者 ====================

/// 通用泛型消费者
///
/// 提供消费者的通用骨架逻辑，具体 topic 消费者只需填充泛型参数。
pub struct GenericConsumer<E, F, H>
where
    E: Event + Send + Sync,
    F: MessageFetcher<E> + Send + Sync + 'static,
    H: MessageHandler<E> + Send + Sync + 'static,
{
    topic: String,
    config: TopicConsumerConfig,
    fetcher: F,
    handler: H,
    _phantom: PhantomData<E>,
}

impl<E, F, H> GenericConsumer<E, F, H>
where
    E: Event + Send + Sync,
    F: MessageFetcher<E> + Send + Sync + 'static,
    H: MessageHandler<E> + Send + Sync + 'static,
{
    /// 创建消费者实例
    pub fn new(
        topic: &str,
        config: TopicConsumerConfig,
        fetcher: F,
        handler: H,
    ) -> Self {
        Self {
            topic: topic.to_string(),
            config,
            fetcher,
            handler,
            _phantom: PhantomData,
        }
    }

    /// 启动消费者（永久运行）
    pub async fn start(self: Arc<Self>) {
        let concurrency = self.config.concurrency.unwrap_or(1);

        // 启动 N 个 worker
        for worker_id in 0..concurrency {
            let consumer = self.clone();
            tokio::spawn(async move {
                consumer.run_worker(worker_id).await;
            });
        }
    }

    /// 单个 worker 循环
    async fn run_worker(&self, worker_id: usize) {
        tracing::info!("[{}] worker {} started", self.topic, worker_id);

        loop {
            match self.fetcher.dequeue_next().await {
                Ok(Some(event)) => {
                    // TODO: 获取 event.id() 需要 E 实现 Event trait
                    // 暂时不处理，后续完善
                    let _ = self.handler.handle(&event).await;
                }
                Ok(None) => {
                    // 队列为空，休眠
                    self.sleep_empty().await;
                }
                Err(e) => {
                    tracing::error!(
                        "[{}] worker {} dequeue error: {}",
                        self.topic,
                        worker_id,
                        e
                    );
                    self.sleep_on_error().await;
                }
            }
        }
    }

    /// 队列为空时休眠
    async fn sleep_empty(&self) {
        let ms = self.config.empty_queue_sleep_ms.unwrap_or(100);
        tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
    }

    /// 出错时休眠
    async fn sleep_on_error(&self) {
        let ms = self.config.error_retry_sleep_ms.unwrap_or(1000);
        tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
    }
}

// ==================== 总初始化入口 ====================

/// 初始化所有消费者
///
/// 由 main.rs 调用，传入全局消费者配置。
pub async fn init(config: &common::config::ConsumerConfig) -> Result<()> {
    tracing::info!("initializing consumers...");

    // 初始化 message topic 消费者
    message::init(&config.for_topic("message")).await?;

    // 未来其他 topic 消费者在这里初始化...

    tracing::info!("all consumers initialized and started");
    Ok(())
}

// ==================== 测试模块 ====================

#[cfg(test)]
mod tests;          // 通用消费者框架测试

#[cfg(test)]
mod message_tests;  // Message Topic 消费者测试
