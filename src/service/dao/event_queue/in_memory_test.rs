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
use std::time::{SystemTime, UNIX_EPOCH};

/// 测试空队列基本操作
#[tokio::test]
async fn test_event_queue_empty() {
    // 创建一个空池用于测试（实际不使用）
    // InMemoryEventQueue 不碰数据库，只是占位
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::<Message>::new();

    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
    assert_eq!(queue.in_progress_count(), 0);

    // 空队列 dequeue 返回 None
    let result = queue.dequeue_next(ctx.clone());
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

/// 测试单个事件入队出队 ack
#[tokio::test]
async fn test_single_event_enqueue_dequeue_ack() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::<Message>::new();

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
    let result = queue.enqueue(ctx.clone(), Box::new(msg.clone()));
    assert!(result.is_ok());
    assert!(!queue.is_empty());
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.in_progress_count(), 0);

    // 出队
    let event_opt = queue.dequeue_next(ctx.clone()).unwrap();
    assert!(event_opt.is_some());
    let event = event_opt.unwrap();
    assert_eq!(event.id(), msg.id());
    assert_eq!(queue.len(), 1); // 出队后还在，只是标记处理中
    assert_eq!(queue.in_progress_count(), 1);

    // ack 确认
    let ack_result = queue.ack(ctx.clone(), event.id());
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

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn into_any(self: Box<TestEvent>) -> Box<dyn std::any::Any> {
            Box::new(self)
        }

        fn id(&self) -> &str {
            &self.id
        }
        fn topic(&self) -> crate::models::event::EventTopic {
            crate::models::event::EventTopic::Message
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
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
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

    let queue = EventQueueDaoInMemoryImpl::<TestEvent>::new();

    // 按低、中、高顺序入队
    queue.enqueue(ctx.clone(), Box::new(low)).unwrap();
    queue.enqueue(ctx.clone(), Box::new(medium)).unwrap();
    queue.enqueue(ctx.clone(), Box::new(high)).unwrap();
    // 出队顺序应该是高 → 中 → 低
    assert_eq!(queue.len(), 3);

    let first = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(first.id(), "high");
    queue.ack(ctx.clone(), first.id()).unwrap();

    let second = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(second.id(), "medium");
    queue.ack(ctx.clone(), second.id()).unwrap();

    let third = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(third.id(), "low");
    queue.ack(ctx.clone(), third.id()).unwrap();

    assert!(queue.is_empty());
}

/// 测试同创建时间，优先级高先出队
#[tokio::test]
async fn test_same_time_priority_ordering() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);

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

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn into_any(self: Box<TestEvent>) -> Box<dyn std::any::Any> {
            Box::new(self)
        }

        fn id(&self) -> &str {
            &self.id
        }
        fn topic(&self) -> crate::models::event::EventTopic {
            crate::models::event::EventTopic::Message
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
    }

    let queue = EventQueueDaoInMemoryImpl::<TestEvent>::new();

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

    queue.enqueue(ctx.clone(), Box::new(low)).unwrap();
    queue.enqueue(ctx.clone(), Box::new(high)).unwrap();

    let first = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(first.id(), "high");
}

/// 测试同优先级，创建时间早的先出队
#[tokio::test]
async fn test_same_priority_time_ordering() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);

    #[derive(Debug, Clone)]
    struct TestEvent {
        id: String,
        priority: u8,
        created_at: i64,
        order_key: String,
    }

    let queue = EventQueueDaoInMemoryImpl::<TestEvent>::new();

    impl Event for TestEvent {
        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn into_any(self: Box<TestEvent>) -> Box<dyn std::any::Any> {
            Box::new(self)
        }

        fn id(&self) -> &str {
            &self.id
        }
        fn topic(&self) -> crate::models::event::EventTopic {
            crate::models::event::EventTopic::Message
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

    queue.enqueue(ctx.clone(), Box::new(late)).unwrap();
    queue.enqueue(ctx.clone(), Box::new(early)).unwrap();

    // 尽管 early 后入队，但创建早，应该先出队
    let first = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(first.id(), "early");
}

/// 测试相同 order_key 保证顺序消费
#[tokio::test]
async fn test_same_order_key_sequential() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);

    #[derive(Debug, Clone)]
    struct TestEvent {
        id: String,
        created_at: i64,
    }

    let queue = EventQueueDaoInMemoryImpl::<TestEvent>::new();

    impl Event for TestEvent {
        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn into_any(self: Box<TestEvent>) -> Box<dyn std::any::Any> {
            Box::new(self)
        }

        fn id(&self) -> &str {
            &self.id
        }
        fn topic(&self) -> crate::models::event::EventTopic {
            crate::models::event::EventTopic::Message
        }
        fn order_key(&self) -> &str {
            "task-001" // 所有事件同 order_key
        }
        fn created_at(&self) -> i64 {
            self.created_at
        }
    }

    // 按顺序入队 1、2、3
    let e1 = TestEvent { id: "1".to_string(), created_at: 1 };
    let e2 = TestEvent { id: "2".to_string(), created_at: 2 };
    let e3 = TestEvent { id: "3".to_string(), created_at: 3 };

    queue.enqueue(ctx.clone(), Box::new(e1)).unwrap();
    queue.enqueue(ctx.clone(), Box::new(e2)).unwrap();
    queue.enqueue(ctx.clone(), Box::new(e3)).unwrap();

    assert_eq!(queue.len(), 3);

    // 第一个出队，必须是 1
    let first = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(first.id(), "1");
    assert_eq!(queue.in_progress_count(), 1);

    // 新的正确行为：出队后不 refill，此时 dequeue_next 应该返回 None
    let second_opt = queue.dequeue_next(ctx.clone()).unwrap();
    assert!(second_opt.is_none()); // 还没 ack，第二个不会被 refill

    // ack 第一个，触发 refill 第二个
    queue.ack(ctx.clone(), "1").unwrap();
    assert_eq!(queue.in_progress_count(), 0); // 第一个已完成

    // 现在可以出队第二个了
    let second = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(second.id(), "2");
    assert_eq!(queue.in_progress_count(), 1);

    // ack 第二个，触发 refill 第三个
    queue.ack(ctx.clone(), "2").unwrap();

    let third = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(third.id(), "3");
    queue.ack(ctx.clone(), "3").unwrap();

    assert!(queue.is_empty());
}

/// 测试 nack 重试
#[tokio::test]
async fn test_nack_retry() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::<Message>::new();

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

    queue.enqueue(ctx.clone(), Box::new(msg.clone())).unwrap();
    assert_eq!(queue.len(), 1);

    // 出队
    let event = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(event.id(), msg.id());
    assert_eq!(queue.in_progress_count(), 1);

    // nack，不删除，重新入队
    queue.nack(ctx.clone(), event.id()).unwrap();
    assert_eq!(queue.in_progress_count(), 0);
    assert_eq!(queue.len(), 1); // 仍然存在

    // 可以再次出队
    let event2 = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(event2.id(), msg.id());
    // ack 确认
    queue.ack(ctx.clone(), event2.id()).unwrap();
    assert!(queue.is_empty());
}

/// 测试：同 order_key 的消息入队时，前一个正在处理的情况
/// 验证：消息 A 正在处理时，消息 B 入队不会被放到全局堆
#[tokio::test]
async fn test_same_order_key_while_processing() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);

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

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn into_any(self: Box<TestEvent>) -> Box<dyn std::any::Any> {
            Box::new(self)
        }

        fn id(&self) -> &str {
            &self.id
        }
        fn topic(&self) -> crate::models::event::EventTopic {
            crate::models::event::EventTopic::Message
        }
        fn order_key(&self) -> &str {
            &self.order_key
        }
        fn priority(&self) -> u8 {
            0
        }
        fn created_at(&self) -> i64 {
            self.created_at
        }
    }

    let queue = EventQueueDaoInMemoryImpl::<TestEvent>::new();

    // 消息 A 入队，出队，开始处理
    queue.enqueue(ctx.clone(), Box::new(TestEvent {
        id: "msg-A".to_string(),
        order_key: "task1".to_string(),
        created_at: 1,
    })).unwrap();

    let a = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(a.id(), "msg-A");
    assert_eq!(queue.in_progress_count(), 1);

    // 消息 B 入队（A 正在处理中）
    queue.enqueue(ctx.clone(), Box::new(TestEvent {
        id: "msg-B".to_string(),
        order_key: "task1".to_string(),
        created_at: 2,
    })).unwrap();

    // 关键断言：此时全局堆应该是空的，B 不应该在全局堆
    // 因为 A 正在处理，has_waiting_in_global 应该是 true
    let next = queue.dequeue_next(ctx.clone()).unwrap();
    assert!(next.is_none(), "B 应该留在子队列等待，不应该在全局堆");

    // ack A，应该触发 refill B
    queue.ack(ctx.clone(), "msg-A").unwrap();

    // 现在应该可以出队 B 了
    let b = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(b.id(), "msg-B");
}

/// 测试带 order_key 的消息 nack 时的严格顺序保证
/// 验证：同一 order_key 同一时间全局堆最多只有一个消息
#[tokio::test]
async fn test_order_key_nack_strict_ordering() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);

    #[derive(Debug, Clone)]
    struct TestEvent {
        id: String,
        created_at: i64,
    }

    impl Event for TestEvent {
        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn into_any(self: Box<TestEvent>) -> Box<dyn std::any::Any> {
            Box::new(self)
        }

        fn id(&self) -> &str {
            &self.id
        }
        fn topic(&self) -> crate::models::event::EventTopic {
            crate::models::event::EventTopic::Message
        }
        fn order_key(&self) -> &str {
            "same-task" // 所有事件同 order_key
        }
        fn created_at(&self) -> i64 {
            self.created_at
        }
    }

    let queue = EventQueueDaoInMemoryImpl::<TestEvent>::new();

    // 按顺序入队 A、B、C
    let a = TestEvent { id: "A".to_string(), created_at: 1 };
    let b = TestEvent { id: "B".to_string(), created_at: 2 };
    let c = TestEvent { id: "C".to_string(), created_at: 3 };

    queue.enqueue(ctx.clone(), Box::new(a)).unwrap();
    queue.enqueue(ctx.clone(), Box::new(b)).unwrap();
    queue.enqueue(ctx.clone(), Box::new(c)).unwrap();

    assert_eq!(queue.len(), 3);

    // 第一个出队必须是 A
    let first = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(first.id(), "A");
    assert_eq!(queue.in_progress_count(), 1);

    // 尝试再次出队，应该是 None（因为 B、C 还在子队列，A 没 ack 不会 refill）
    let second_try = queue.dequeue_next(ctx.clone()).unwrap();
    assert!(second_try.is_none(), "同一 order_key 同一时间只能有一个消息在全局堆");

    // nack A，A 应该直接回到全局堆，B、C 仍在子队列
    queue.nack(ctx.clone(), "A").unwrap();
    assert_eq!(queue.in_progress_count(), 0);

    // 再次出队，应该还是 A（不是 B 也不是 C）
    let after_nack = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(after_nack.id(), "A", "nack 后应该优先处理刚才失败的消息 A");

    // 再次尝试出队，应该还是 None（B、C 仍在子队列）
    let third_try = queue.dequeue_next(ctx.clone()).unwrap();
    assert!(third_try.is_none(), "同一 order_key 同一时间只能有一个消息在全局堆");

    // 现在正常 ack A，触发 refill B
    queue.ack(ctx.clone(), "A").unwrap();

    // 出队应该是 B
    let b_event = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(b_event.id(), "B");

    // ack B，触发 refill C
    queue.ack(ctx.clone(), "B").unwrap();
    let c_event = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(c_event.id(), "C");

    // ack C，队列空
    queue.ack(ctx.clone(), "C").unwrap();
    assert!(queue.is_empty());
}

/// 测试批量入队
#[tokio::test]
async fn test_batch_enqueue() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);
    let queue = EventQueueDaoInMemoryImpl::<Message>::new();

    let mut events: Vec<Box<Message>> = Vec::new();
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

    let result = queue.enqueue_batch(ctx.clone(), events);
    assert!(result.is_ok());
    assert_eq!(queue.len(), 5);

    // 全部出队 ack
    let mut count = 0;
    while let Some(event) = queue.dequeue_next(ctx.clone()).unwrap() {
        count += 1;
        queue.ack(ctx.clone(), event.id()).unwrap();
    }

    assert_eq!(count, 5);
    assert!(queue.is_empty());
}

/// 测试混合不同 order_key 分组
#[tokio::test]
async fn test_mixed_order_groups() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = RequestContext::new_simple("test-user", pool);

    // task1: 3个事件，顺序消费
    // task2: 2个事件，顺序消费
    // 独立并行事件

    #[derive(Debug, Clone)]
    struct TestEvent {
        id: String,
        order_key: String,
        created_at: i64,
    }

    let queue = EventQueueDaoInMemoryImpl::<TestEvent>::new();

    impl Event for TestEvent {
        fn clone_box(&self) -> Box<dyn Event> {
            Box::new(self.clone())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn into_any(self: Box<TestEvent>) -> Box<dyn std::any::Any> {
            Box::new(self)
        }

        fn id(&self) -> &str {
            &self.id
        }
        fn topic(&self) -> crate::models::event::EventTopic {
            crate::models::event::EventTopic::Message
        }
        fn order_key(&self) -> &str {
            &self.order_key
        }
        fn priority(&self) -> u8 {
            0
        }
        fn created_at(&self) -> i64 {
            self.created_at
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
        queue.enqueue(ctx.clone(), Box::new(e)).unwrap();
    }

    assert_eq!(queue.len(), 6);

    // 全局堆初始有：t1-1 (created_at=1), t2-1 (created_at=4), parallel (created_at=6)
    // 每个 order_key 的第一条消息入队时就被放到全局堆（只要该 order_key 还没有消息在全局堆）

    // 第一个出队是 t1-1（created_at 最早）
    let first = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(first.id(), "t1-1");

    // t1-1 出队后不 refill，下一个是 t2-1（created_at=4）
    let second = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(second.id(), "t2-1");

    // t2-1 出队后不 refill，下一个是 parallel（created_at=6）
    let third = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(third.id(), "parallel");

    // 现在全局堆空了
    let fourth = queue.dequeue_next(ctx.clone()).unwrap();
    assert!(fourth.is_none());

    // ack t1-1，触发 refill t1-2
    queue.ack(ctx.clone(), "t1-1").unwrap();
    let fifth = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(fifth.id(), "t1-2");

    // dequeue 又空了
    let sixth = queue.dequeue_next(ctx.clone()).unwrap();
    assert!(sixth.is_none());

    // ack t1-2，触发 refill t1-3
    queue.ack(ctx.clone(), "t1-2").unwrap();
    let seventh = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(seventh.id(), "t1-3");
    queue.ack(ctx.clone(), "t1-3").unwrap();

    // ack t2-1，触发 refill t2-2
    queue.ack(ctx.clone(), "t2-1").unwrap();
    let eighth = queue.dequeue_next(ctx.clone()).unwrap().unwrap();
    assert_eq!(eighth.id(), "t2-2");
    queue.ack(ctx.clone(), "t2-2").unwrap();

    // ack parallel
    queue.ack(ctx.clone(), "parallel").unwrap();

    assert!(queue.is_empty());
}
