//! Message DAL 单元测试

use crate::models::file::FileMeta;
use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::dao::event_queue;
use crate::service::dao::message;
use common::enums::{MessageRole, MessageStatus, MessageType};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// 创建测试消息
fn create_test_message(
    task_id: &str,
    from_id: &str,
    to_id: &str,
    from_role: MessageRole,
    to_role: MessageRole,
    content: String,
) -> Message {
    let id = Uuid::now_v7().to_string();
    let file_meta = FileMeta::default();
    Message::new_with_context(
        id,
        None,
        Some(task_id.to_string()),
        from_id.to_string(),
        to_id.to_string(),
        from_role,
        to_role,
        MessageType::Text,
        content,
        None,
        file_meta,
        None,
        from_id.to_string(),
    )
}

#[sqlx::test]
async fn test_save_and_find_by_id(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    let msg = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Hello world".to_string(),
    );

    dal.save_message(ctx.clone(), &msg).await.unwrap();
    let found = dal.find_by_id(ctx, msg.id()).await.unwrap();

    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.po.content, "Hello world");
    assert_eq!(found.task_id(), Some("task-1"));
    assert_eq!(found.po.from_role, MessageRole::User);
    assert_eq!(found.po.to_role, MessageRole::Agent);
    assert_eq!(found.po.status, MessageStatus::Pending);
}

#[sqlx::test]
async fn test_list_by_task_id(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    // Add messages to two different tasks
    for i in 0..5 {
        let msg = create_test_message(
            "task-1",
            "user-1",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("Message {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    for i in 0..3 {
        let msg = create_test_message(
            "task-2",
            "user-1",
            "agent-2",
            MessageRole::User,
            MessageRole::Agent,
            format!("Other {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let list = dal.list_by_task_id(ctx.clone(), "task-1", None).await.unwrap();
    assert_eq!(list.len(), 5);
    // Check order: created_at ASC
    for (i, msg) in list.iter().enumerate() {
        assert_eq!(msg.po.content, format!("Message {}", i));
    }

    let list2 = dal.list_by_task_id(ctx, "task-2", None).await.unwrap();
    assert_eq!(list2.len(), 3);
}

#[sqlx::test]
async fn test_list_by_task_id_with_limit(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    for i in 0..10 {
        let msg = create_test_message(
            "task-1",
            "user-1",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("Message {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let list = dal.list_by_task_id(ctx, "task-1", Some(5)).await.unwrap();
    assert_eq!(list.len(), 5);
    // First 5 messages in order
    for (i, msg) in list.iter().enumerate() {
        assert_eq!(msg.po.content, format!("Message {}", i));
    }
}

#[sqlx::test]
async fn test_list_by_from_id(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool.clone());

    for i in 0..3 {
        let msg = create_test_message(
            "task-1",
            "user-alice",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("From Alice {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let msg2 = create_test_message(
        "task-1",
        "user-bob",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "From Bob".to_string(),
    );
    dal.save_message(ctx, &msg2).await.unwrap();

    let ctx = RequestContext::new_simple("admin", pool.clone());
    let list = dal.list_by_from_id(ctx, "user-alice", None).await.unwrap();
    assert_eq!(list.len(), 3);
}

#[sqlx::test]
async fn test_list_by_to_id(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    for i in 0..4 {
        let msg = create_test_message(
            "task-1",
            "user-1",
            "agent-alice",
            MessageRole::User,
            MessageRole::Agent,
            format!("To Alice {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let list = dal.list_by_to_id(ctx, "agent-alice", None).await.unwrap();
    assert_eq!(list.len(), 4);
}

#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    let mut msg1 = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Pending".to_string(),
    );
    let mut msg2 = create_test_message(
        "task-1",
        "agent-1",
        "user-1",
        MessageRole::Agent,
        MessageRole::User,
        "Processed".to_string(),
    );
    msg1.po.status = MessageStatus::Pending;
    msg2.po.status = MessageStatus::Processed;

    dal.save_message(ctx.clone(), &msg1).await.unwrap();
    dal.save_message(ctx.clone(), &msg2).await.unwrap();

    let pending = dal.list_by_status(ctx.clone(), vec![MessageStatus::Pending], None).await.unwrap();
    assert_eq!(pending.len(), 1);

    let processed = dal.list_by_status(ctx.clone(), vec![MessageStatus::Processed], None).await.unwrap();
    assert_eq!(processed.len(), 1);

    let both = dal.list_by_status(ctx, vec![MessageStatus::Pending, MessageStatus::Processed], None).await.unwrap();
    assert_eq!(both.len(), 2);
}

#[sqlx::test]
async fn test_update_status(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    let mut msg = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Hello".to_string(),
    );
    msg.po.status = MessageStatus::Pending;
    dal.save_message(ctx.clone(), &msg).await.unwrap();

    let found = dal.find_by_id(ctx.clone(), msg.id()).await.unwrap().unwrap();
    assert_eq!(found.po.status, MessageStatus::Pending);

    dal.update_status(ctx.clone(), msg.id(), MessageStatus::Processed).await.unwrap();

    let found = dal.find_by_id(ctx, msg.id()).await.unwrap().unwrap();
    assert_eq!(found.po.status, MessageStatus::Processed);
}

#[sqlx::test]
async fn test_count_by_task_id(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    for i in 0..7 {
        let msg = create_test_message(
            "task-counter",
            "user-1",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("Msg {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let count = dal.count_by_task_id(ctx.clone(), "task-counter").await.unwrap();
    assert_eq!(count, 7);

    let count2 = dal.count_by_task_id(ctx, "empty-task").await.unwrap();
    assert_eq!(count2, 0);
}

#[sqlx::test]
async fn test_delete_message(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    let msg = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "To be deleted".to_string(),
    );
    dal.save_message(ctx.clone(), &msg).await.unwrap();

    let found = dal.find_by_id(ctx.clone(), msg.id()).await.unwrap();
    assert!(found.is_some());

    dal.delete_message(ctx.clone(), msg.id()).await.unwrap();

    let found = dal.find_by_id(ctx, msg.id()).await.unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_delete_by_task_id(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    for i in 0..5 {
        let msg = create_test_message(
            "task-to-delete",
            "user-1",
            "agent-1",
            MessageRole::User,
            MessageRole::Agent,
            format!("Msg {}", i),
        );
        dal.save_message(ctx.clone(), &msg).await.unwrap();
    }

    let count = dal.count_by_task_id(ctx.clone(), "task-to-delete").await.unwrap();
    assert_eq!(count, 5);

    dal.delete_by_task_id(ctx.clone(), "task-to-delete").await.unwrap();

    let count = dal.count_by_task_id(ctx, "task-to-delete").await.unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    let found = dal.find_by_id(ctx, "not-existent-id").await.unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_dequeue_ack_nack(pool: SqlitePool) {
    let message_dao = message::sqlite::new();
    let event_queue = event_queue::in_memory::new();
    let dal = crate::service::dal::message::new(message_dao, event_queue);
    let ctx = RequestContext::new_simple("admin", pool);

    // 空队列第一次 dequeue 应该从 DB 加载
    let msg1 = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Message 1".to_string(),
    );
    let msg2 = create_test_message(
        "task-1",
        "agent-1",
        "user-1",
        MessageRole::Agent,
        MessageRole::User,
        "Message 2".to_string(),
    );
    let msg3 = create_test_message(
        "task-1",
        "user-1",
        "agent-1",
        MessageRole::User,
        MessageRole::Agent,
        "Message 3".to_string(),
    );

    // 保存三条消息，都会自动入队
    dal.save_message(ctx.clone(), &msg1).await.unwrap();
    dal.save_message(ctx.clone(), &msg2).await.unwrap();
    dal.save_message(ctx.clone(), &msg3).await.unwrap();

    // 第一次出队：队列为空，会自动回源 DB 加载，返回第一条按时间排序
    let dequeued1 = dal.dequeue_next_message(ctx.clone()).await.unwrap();
    assert!(dequeued1.is_some());
    let dequeued1 = dequeued1.unwrap();
    assert_eq!(dequeued1.po.content, "Message 1");

    // 第二次出队：内存队列还有，直接返回第二条
    let dequeued2 = dal.dequeue_next_message(ctx.clone()).await.unwrap();
    assert!(dequeued2.is_some());
    let dequeued2 = dequeued2.unwrap();
    assert_eq!(dequeued2.po.content, "Message 2");

    // 第三次出队：返回第三条
    let dequeued3 = dal.dequeue_next_message(ctx.clone()).await.unwrap();
    assert!(dequeued3.is_some());
    let dequeued3 = dequeued3.unwrap();
    assert_eq!(dequeued3.po.content, "Message 3");

    // 第四次出队：内存队列空了，DB 也没有 pending 了，返回 None
    let dequeued4 = dal.dequeue_next_message(ctx.clone()).await.unwrap();
    if let Some(msg) = &dequeued4 {
        println!("dequeued4 content: {}", msg.po.content);
    }
    assert!(dequeued4.is_none());

    // ack 第一个消息
    dal.ack_message(ctx.clone(), dequeued1.id()).await.unwrap();
    // 更新消息状态为 Processed
    dal.update_status(ctx.clone(), dequeued1.id(), MessageStatus::Processed).await.unwrap();

    // nack 第二个消息（处理失败，放回队列）
    dal.nack_message(ctx.clone(), dequeued2.id()).await.unwrap();

    // 再次出队应该能拿到被 nack 的消息
    let dequeued5 = dal.dequeue_next_message(ctx.clone()).await.unwrap();
    assert!(dequeued5.is_some());
    let dequeued5 = dequeued5.unwrap();
    assert_eq!(dequeued5.po.content, "Message 2");
}
