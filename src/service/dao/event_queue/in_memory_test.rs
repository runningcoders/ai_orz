//! EventQueue DAO 单元测试
//!
//! InMemoryEventQueue 纯内存实现测试

use super::*;
use crate::models::event::Event;
use crate::models::file::FileMeta;
use crate::models::message::Message;
use common::enums::{MessageRole, MessageType, FileType};
use crate::pkg::RequestContext;
use crate::service::dao::event_queue::in_memory::EventQueueDaoInMemoryImpl;
use sqlx::SqlitePool;

/// 测试空队列基本操作
#[tokio::test]
async fn test_event_queue_empty() {
    // 创建一个空池用于测试（实际不使用）
    // InMemoryEventQueue 不碰数据库，只是占位
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::new();

    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
    assert_eq!(queue.in_progress_count(), 0);

    // 空队列 dequeue 返回 None
    let result = queue.dequeue_next(&ctx);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

/// 测试单个事件入队出队 ack
#[tokio::test]
async fn test_single_event_enqueue_dequeue_ack() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::new();

    // 创建一个测试消息
    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );
    let msg = Message::new(
        uuid::Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "agent-001".to_string(),
        MessageRole::User,
        MessageRole::Agent, // to_role
        MessageType::Text,
        "测试消息".to_string(),
        None,
        empty_file_meta,
        "test-user".to_string(),
    );

    // 入队
    let result = queue.enqueue(&ctx, Box::new(msg.clone()));
    assert!(result.is_ok());
    assert!(!queue.is_empty());
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.in_progress_count(), 0);

    // 出队
    let event_opt = queue.dequeue_next(&ctx).unwrap();
    assert!(event_opt.is_some());
    let event = event_opt.unwrap();
    assert_eq!(event.id(), msg.id());
    assert_eq!(queue.len(), 1); // 出队后还在，只是标记处理中
    assert_eq!(queue.in_progress_count(), 1);

    // ack 确认
    let ack_result = queue.ack(&ctx, event.id());
    assert!(ack_result.is_ok());
    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
    assert_eq!(queue.in_progress_count(), 0);
}

/// 测试优先级排序 - 高优先级先出队
#[tokio::test]
async fn test_priority_ordering() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::new();

    // 创建三个不同优先级的事件，优先级低的先入队
    #[derive(Debug, Clone)]
    struct TestEvent {
        id: String,
        priority: u8,
        created_at: i64,
        order_key: String,
    }

    impl Event for TestEvent {
        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }
        fn id(&self) -> &str {
            &self.id
        }
        fn event_type(&self) -> crate::models::event::EventType {
            crate::models::event::EventType::Message
        }
        fn order_key(&self) -> &str {
            &self.order_key
        }
        fn priority(&self) -> u8 {
            self.priority
        }
        fn created_at(&self) -> i64 {
            self.created_at
        }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
                self
            }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let low = TestEvent {
        id: "low".to_string(),
        priority: 1,
        created_at: now - 3,
        order_key: "".to_string(),
    };
    let medium = TestEvent {
        id: "medium".to_string(),
        priority: 5,
        created_at: now - 2,
        order_key: "".to_string(),
    };
    let high = TestEvent {
        id: "high".to_string(),
        priority: 9,
        created_at: now - 1,
        order_key: "".to_string(),
    };

    // 按低、中、高顺序入队
    queue.enqueue(&ctx, Box::new(low)).unwrap();
    queue.enqueue(&ctx, Box::new(medium)).unwrap();
    queue.enqueue(&ctx, Box::new(high)).unwrap();

    // 出队顺序应该是高 → 中 → 低
    assert_eq!(queue.len(), 3);

    let first = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(first.id(), "high");
    queue.ack(&ctx, first.id()).unwrap();

    let second = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(second.id(), "medium");
    queue.ack(&ctx, second.id()).unwrap();

    let third = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(third.id(), "low");
    queue.ack(&ctx, third.id()).unwrap();

    assert!(queue.is_empty());
}

/// 测试同创建时间，优先级高先出队
#[tokio::test]
async fn test_same_time_priority_ordering() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::new();

    #[derive(Debug, Clone)]
    struct TestEvent {
        id: String,
        priority: u8,
        created_at: i64,
        order_key: String,
    }

    impl Event for TestEvent {
        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }
        fn id(&self) -> &str {
            &self.id
        }
        fn event_type(&self) -> crate::models::event::EventType {
            crate::models::event::EventType::Message
        }
        fn order_key(&self) -> &str {
            &self.order_key
        }
        fn priority(&self) -> u8 {
            self.priority
        }
        fn created_at(&self) -> i64 {
            self.created_at
        }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
                self
            }
    }

    let now = 1000;

    let low = TestEvent {
        id: "low".to_string(),
        priority: 1,
        created_at: now,
        order_key: "".to_string(),
    };
    let high = TestEvent {
        id: "high".to_string(),
        priority: 9,
        created_at: now,
        order_key: "".to_string(),
    };

    queue.enqueue(&ctx, Box::new(low)).unwrap();
    queue.enqueue(&ctx, Box::new(high)).unwrap();

    let first = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(first.id(), "high");
}

/// 测试同优先级，创建时间早的先出队
#[tokio::test]
async fn test_same_priority_time_ordering() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::new();

    #[derive(Debug, Clone)]
    struct TestEvent {
        id: String,
        priority: u8,
        created_at: i64,
        order_key: String,
    }

    impl Event for TestEvent {
        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }
        fn id(&self) -> &str {
            &self.id
        }
        fn event_type(&self) -> crate::models::event::EventType {
            crate::models::event::EventType::Message
        }
        fn order_key(&self) -> &str {
            &self.order_key
        }
        fn priority(&self) -> u8 {
            self.priority
        }
        fn created_at(&self) -> i64 {
            self.created_at
        }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
                self
            }
    }

    let early = TestEvent {
        id: "early".to_string(),
        priority: 5,
        created_at: 1000,
        order_key: "".to_string(),
    };
    let late = TestEvent {
        id: "late".to_string(),
        priority: 5,
        created_at: 2000,
        order_key: "".to_string(),
    };

    queue.enqueue(&ctx, Box::new(late)).unwrap();
    queue.enqueue(&ctx, Box::new(early)).unwrap();

    // 尽管 early 后入队，但创建早，应该先出队
    let first = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(first.id(), "early");
}

/// 测试相同 order_key 保证顺序消费
#[tokio::test]
async fn test_same_order_key_sequential() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::new();

    #[derive(Debug, Clone)]
    struct TestEvent {
        id: String,
        created_at: i64,
    }

    impl Event for TestEvent {
        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }
        fn id(&self) -> &str {
            &self.id
        }
        fn event_type(&self) -> crate::models::event::EventType {
            crate::models::event::EventType::Message
        }
        fn order_key(&self) -> &str {
            "task-001" // 所有事件同 order_key
        }
        fn created_at(&self) -> i64 {
            self.created_at
        }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
                self
            }
    }

    // 按顺序入队 1、2、3
    let e1 = TestEvent { id: "1".to_string(), created_at: 1 };
    let e2 = TestEvent { id: "2".to_string(), created_at: 2 };
    let e3 = TestEvent { id: "3".to_string(), created_at: 3 };

    queue.enqueue(&ctx, Box::new(e1)).unwrap();
    queue.enqueue(&ctx, Box::new(e2)).unwrap();
    queue.enqueue(&ctx, Box::new(e3)).unwrap();

    assert_eq!(queue.len(), 3);

    // 第一个出队，必须是 1
    let first = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(first.id(), "1");
    assert_eq!(queue.in_progress_count(), 1);
    // 第一个出队后，第二个已经 refill 到全局堆，可以直接出队
    // 同一 order_key 同一时间只有一个在处理中，满足顺序处理要求
    let second_opt = queue.dequeue_next(&ctx).unwrap();
    assert!(second_opt.is_some()); // 第二个已经 refill 到全局堆
    let second = second_opt.unwrap();
    assert_eq!(second.id(), "2");
    assert_eq!(queue.in_progress_count(), 2); // 现在第二个也出队了，两个都在处理中？不，不对，我们的设计是只允许一个 in_progress

    // ack 第一个，第二个已经出队在等待处理
    queue.ack(&ctx, "1").unwrap();
    assert_eq!(queue.in_progress_count(), 1); // 只有第二个在处理中
    queue.ack(&ctx, "2").unwrap();

    let third = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(third.id(), "3");
    queue.ack(&ctx, "3").unwrap();

    assert!(queue.is_empty());
}

/// 测试 nack 重试
#[tokio::test]
async fn test_nack_retry() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::new();

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );
    let msg = Message::new(
        uuid::Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "agent-001".to_string(),
        MessageRole::User,
        MessageRole::Agent, // to_role
        MessageType::Text,
        "测试 nack".to_string(),
        None,
        empty_file_meta,
        "test-user".to_string(),
    );

    queue.enqueue(&ctx, Box::new(msg.clone())).unwrap();
    assert_eq!(queue.len(), 1);

    // 出队
    let event = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(event.id(), msg.id());
    assert_eq!(queue.in_progress_count(), 1);

    // nack，不删除，重新入队
    queue.nack(&ctx, event.id()).unwrap();
    assert_eq!(queue.in_progress_count(), 0);
    assert_eq!(queue.len(), 1); // 仍然存在

    // 可以再次出队
    let event2 = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(event2.id(), msg.id());
    // ack 确认
    queue.ack(&ctx, event2.id()).unwrap();
    assert!(queue.is_empty());
}

/// 测试批量入队
#[tokio::test]
async fn test_batch_enqueue() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::new();

    let mut events: Vec<Box<dyn Event>> = Vec::new();
    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );
    for i in 0..5 {
        let msg = Message::new(
            uuid::Uuid::now_v7().to_string(),
            format!("batch-task-{}", i),
            "user-001".to_string(),
            "agent-001".to_string(),
            MessageRole::User,
            MessageRole::Agent, // to_role
            MessageType::Text,
            format!("批量消息 {}", i),
            None,
            empty_file_meta.clone(),
            "test-user".to_string(),
        );
        events.push(Box::new(msg));
    }

    let result = queue.enqueue_batch(&ctx, events);
    assert!(result.is_ok());
    assert_eq!(queue.len(), 5);

    // 全部出队 ack
    let mut count = 0;
    while let Some(event) = queue.dequeue_next(&ctx).unwrap() {
        count += 1;
        queue.ack(&ctx, event.id()).unwrap();
    }

    assert_eq!(count, 5);
    assert!(queue.is_empty());
}

/// 测试混合不同 order_key 分组
#[tokio::test]
async fn test_mixed_order_groups() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::new();

    // task1: 3个事件，顺序消费
    // task2: 2个事件，顺序消费
    // 独立并行事件

    #[derive(Debug, Clone)]
    struct TestEvent {
        id: String,
        order_key: String,
        created_at: i64,
    }

    impl Event for TestEvent {
        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }
        fn id(&self) -> &str {
            &self.id
        }
        fn event_type(&self) -> crate::models::event::EventType {
            crate::models::event::EventType::Message
        }
        fn order_key(&self) -> &str {
            &self.order_key
        }
        fn created_at(&self) -> i64 {
            self.created_at
        }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
                self
            }
    }

    let events = vec![
        TestEvent { id: "t1-1".to_string(), order_key: "task1".to_string(), created_at: 1 },
        TestEvent { id: "t1-2".to_string(), order_key: "task1".to_string(), created_at: 2 },
        TestEvent { id: "t1-3".to_string(), order_key: "task1".to_string(), created_at: 3 },
        TestEvent { id: "t2-1".to_string(), order_key: "task2".to_string(), created_at: 4 },
        TestEvent { id: "t2-2".to_string(), order_key: "task2".to_string(), created_at: 5 },
        TestEvent { id: "parallel".to_string(), order_key: "".to_string(), created_at: 6 },
    ];

    for e in events {
        queue.enqueue(&ctx, Box::new(e)).unwrap();
    }

    assert_eq!(queue.len(), 6);

    // 第一个出队应该是 t1-1（创建时间最早）
    let first = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(first.id(), "t1-1");
    // t1-1 出队后，t1-2 自动 refill 到全局堆，t1-2 created_at 更早，所以第二个出队是 t1-2
    let second = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(second.id(), "t1-2");
    // t1-2 出队后，t1-3 自动 refill 到全局堆，第三个出队是 t1-3
    let third = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(third.id(), "t1-3");
    // t1 全部已出队，不再 refill，接下来是 t2-1（created_at 4）
    let fourth = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(fourth.id(), "t2-1");
    // t2-1 出队后，t2-2 自动 refill，第五个出队是 t2-2
    let fifth = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(fifth.id(), "t2-2");
    // t2 全部已出队，最后是 parallel
    let sixth = queue.dequeue_next(&ctx).unwrap().unwrap();
    assert_eq!(sixth.id(), "parallel");
    // 队列空了（全部都已经出队到 in_progress）
    // 现在所有事件都已经出队了（自动 refill 把同组都提前出队到全局堆）
    // 只剩下 ack 流程
    // ack t1-1
    queue.ack(&ctx, "t1-1").unwrap();
    // ack t1-2
    queue.ack(&ctx, "t1-2").unwrap();
    // ack t1-3
    queue.ack(&ctx, "t1-3").unwrap();
    // ack t2-1
    queue.ack(&ctx, "t2-1").unwrap();
    // ack t2-2
    queue.ack(&ctx, "t2-2").unwrap();
    // ack parallel → 全部完成
    queue.ack(&ctx, "parallel").unwrap();

    assert!(queue.is_empty());
}
