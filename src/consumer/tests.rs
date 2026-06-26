//! 通用消费者框架测试
//!
//! 存放与具体 Topic 无关的通用消费者逻辑测试：
//! - 空队列行为
//! - 正常消费流程（拉取 -> 处理 -> ack）
//! - 处理失败流程（拉取 -> 处理失败 -> nack）
//! - 多条消息顺序消费
//!
//! 使用 Mock 方式测试 GenericConsumer 的核心逻辑，不依赖真实数据库。

use super::MessageHandler;
use super::*;
use async_trait::async_trait;
use std::sync::Mutex;
use uuid::Uuid;

// ==================== Mock 实现 ====================

/// Mock 事件：简化的 Event 实现用于测试
#[derive(Debug, Clone)]
struct MockEvent {
    id: String,
    content: String,
}

impl crate::models::event::Event for MockEvent {
    fn clone_box(&self) -> Box<dyn crate::models::event::Event> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn topic(&self) -> crate::models::event::EventTopic {
        crate::models::event::EventTopic::Message
    }

    fn order_key(&self) -> &str {
        ""
    }

    fn priority(&self) -> u8 {
        5
    }

    fn created_at(&self) -> i64 {
        0
    }
}

/// Mock Fetcher：用于控制队列行为
struct MockFetcher {
    events: Mutex<Vec<MockEvent>>,
    ack_called: Mutex<Vec<String>>,
    nack_called: Mutex<Vec<String>>,
}

impl MockFetcher {
    fn new(events: Vec<MockEvent>) -> Self {
        Self {
            events: Mutex::new(events),
            ack_called: Mutex::new(Vec::new()),
            nack_called: Mutex::new(Vec::new()),
        }
    }

    fn add_event(&self, event: MockEvent) {
        self.events.lock().unwrap().push(event);
    }

    fn get_ack_count(&self) -> usize {
        self.ack_called.lock().unwrap().len()
    }

    fn get_nack_count(&self) -> usize {
        self.nack_called.lock().unwrap().len()
    }

    fn was_acked(&self, event_id: &str) -> bool {
        self.ack_called
            .lock()
            .unwrap()
            .contains(&event_id.to_string())
    }

    fn was_nacked(&self, event_id: &str) -> bool {
        self.nack_called
            .lock()
            .unwrap()
            .contains(&event_id.to_string())
    }
}

#[async_trait]
impl super::MessageFetcher<MockEvent> for MockFetcher {
    async fn dequeue_next(&self) -> common::error::Result<Option<MockEvent>> {
        Ok(self.events.lock().unwrap().pop())
    }

    async fn ack(&self, event_id: &str) -> common::error::Result<()> {
        self.ack_called.lock().unwrap().push(event_id.to_string());
        Ok(())
    }

    async fn nack(&self, event_id: &str) -> common::error::Result<()> {
        self.nack_called.lock().unwrap().push(event_id.to_string());
        Ok(())
    }
}

/// Mock Handler：用于控制处理行为
struct MockHandler {
    should_fail: bool,
    handled_events: Mutex<Vec<String>>,
}

impl MockHandler {
    fn new(should_fail: bool) -> Self {
        Self {
            should_fail,
            handled_events: Mutex::new(Vec::new()),
        }
    }

    fn get_handled_count(&self) -> usize {
        self.handled_events.lock().unwrap().len()
    }

    fn was_handled(&self, event_id: &str) -> bool {
        self.handled_events
            .lock()
            .unwrap()
            .contains(&event_id.to_string())
    }
}

#[async_trait]
impl MessageHandler<MockEvent> for MockHandler {
    async fn handle(&self, event: &MockEvent) -> common::error::Result<()> {
        self.handled_events.lock().unwrap().push(event.id.clone());
        if self.should_fail {
            Err(common::error::Error::internal(
                "mock handle failure".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

fn create_mock_event(content: &str) -> MockEvent {
    MockEvent {
        id: Uuid::now_v7().to_string(),
        content: content.to_string(),
    }
}

// ==================== 测试辅助 ====================

#[cfg(test)]
mod consumer_behavior_tests {
    use super::*;
    use common::config::TopicConsumerConfig;
use common::error::Result;

    #[tokio::test]
    async fn test_empty_queue_returns_false() -> common::error::Result<()> {
        // 空队列
        let fetcher = MockFetcher::new(vec![]);
        let handler = MockHandler::new(false);

        let config = TopicConsumerConfig {
            concurrency: Some(1),
            empty_queue_sleep_ms: Some(10),
            error_retry_sleep_ms: Some(10),
        };

        let consumer = GenericConsumer::new("test", config, fetcher, handler);

        // 空队列消费应该返回 false
        let result = consumer.consume_one().await?;
        assert!(!result);

        Ok(())
    }

    #[tokio::test]
    async fn test_successful_consumption_returns_true() -> common::error::Result<()> {
        // 准备一个消息
        let event = create_mock_event("test message");
        let event_id = event.id.clone();

        let fetcher = MockFetcher::new(vec![event]);
        let handler = MockHandler::new(false);

        let config = TopicConsumerConfig {
            concurrency: Some(1),
            empty_queue_sleep_ms: Some(10),
            error_retry_sleep_ms: Some(10),
        };

        let consumer = GenericConsumer::new("test", config, fetcher, handler);

        // 消费消息
        let result = consumer.consume_one().await?;

        // 验证：成功消费，返回 true
        assert!(result);

        // 验证：handler.handle 被调用
        assert!(consumer.handler.was_handled(&event_id));

        // 验证：ack 被调用
        assert!(consumer.fetcher.was_acked(&event_id));
        assert_eq!(consumer.fetcher.get_ack_count(), 1);
        assert_eq!(consumer.fetcher.get_nack_count(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_consumed_message_removed_from_queue() -> common::error::Result<()> {
        // 准备1条消息
        let event = create_mock_event("test message");
        let fetcher = MockFetcher::new(vec![event]);
        let handler = MockHandler::new(false);

        let config = TopicConsumerConfig {
            concurrency: Some(1),
            empty_queue_sleep_ms: Some(10),
            error_retry_sleep_ms: Some(10),
        };

        let consumer = GenericConsumer::new("test", config, fetcher, handler);

        // 第一次消费 - 应该成功
        let result1 = consumer.consume_one().await?;
        assert!(result1);

        // 第二次消费 - 队列为空
        let result2 = consumer.consume_one().await?;
        assert!(!result2);

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_messages_consumed_sequentially() -> common::error::Result<()> {
        // 准备3条消息
        let event1 = create_mock_event("msg 1");
        let event2 = create_mock_event("msg 2");
        let event3 = create_mock_event("msg 3");

        let fetcher = MockFetcher::new(vec![event1, event2, event3]);
        let handler = MockHandler::new(false);

        let config = TopicConsumerConfig {
            concurrency: Some(1),
            empty_queue_sleep_ms: Some(10),
            error_retry_sleep_ms: Some(10),
        };

        let consumer = GenericConsumer::new("test", config, fetcher, handler);

        // 消费3次 - 都应该成功
        for _ in 0..3 {
            let result = consumer.consume_one().await?;
            assert!(result);
        }

        // 验证：3条消息都被处理了
        assert_eq!(consumer.handler.get_handled_count(), 3);
        assert_eq!(consumer.fetcher.get_ack_count(), 3);

        // 第4次消费 - 队列为空
        let result4 = consumer.consume_one().await?;
        assert!(!result4);

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_failure_calls_nack() -> common::error::Result<()> {
        // 准备1条消息
        let event = create_mock_event("test message");
        let event_id = event.id.clone();

        let fetcher = MockFetcher::new(vec![event]);
        let handler = MockHandler::new(true); // 处理会失败

        let config = TopicConsumerConfig {
            concurrency: Some(1),
            empty_queue_sleep_ms: Some(10),
            error_retry_sleep_ms: Some(10),
        };

        let consumer = GenericConsumer::new("test", config, fetcher, handler);

        // 消费消息（虽然 handler 失败了，但 consume_one 仍然应该返回 Ok(true)）
        let result = consumer.consume_one().await?;
        assert!(result);

        // 验证：handler.handle 被调用了
        assert!(consumer.handler.was_handled(&event_id));
        assert_eq!(consumer.handler.get_handled_count(), 1);

        // 验证：nack 被调用，而不是 ack
        assert!(consumer.fetcher.was_nacked(&event_id));
        assert_eq!(consumer.fetcher.get_nack_count(), 1);
        assert_eq!(consumer.fetcher.get_ack_count(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_add_and_consume() -> common::error::Result<()> {
        // 初始空队列
        let fetcher = MockFetcher::new(vec![]);
        let handler = MockHandler::new(false);

        let config = TopicConsumerConfig {
            concurrency: Some(1),
            empty_queue_sleep_ms: Some(10),
            error_retry_sleep_ms: Some(10),
        };

        let consumer = GenericConsumer::new("test", config, fetcher, handler);

        // 先消费 - 队列为空
        let result1 = consumer.consume_one().await?;
        assert!(!result1);

        // 动态添加消息
        let event = create_mock_event("dynamic message");
        let event_id = event.id.clone();
        consumer.fetcher.add_event(event);

        // 再次消费 - 应该成功
        let result2 = consumer.consume_one().await?;
        assert!(result2);

        // 验证消息被处理
        assert!(consumer.handler.was_handled(&event_id));
        assert!(consumer.fetcher.was_acked(&event_id));

        Ok(())
    }
}
