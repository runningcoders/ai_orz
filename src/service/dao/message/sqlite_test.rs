//! Message DAO SQLite 单元测试

use crate::models::message::MessagePo;
use common::enums::{MessageRole, MessageType, MessageStatus};
use crate::pkg::RequestContext;
use crate::service::dao::message::{self, MessageDaoTrait};
use uuid::Uuid;
use sqlx::SqlitePool;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 测试插入消息和按 ID 查询
#[sqlx::test]
async fn test_insert_and_find_by_id(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let msg = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "你好，这是一条测试消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = message_dao.insert(ctx.clone(), &msg).await;
    assert!(result.is_ok());

    let found = message_dao.find_by_id(ctx.clone(), msg.id.as_str()).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, msg.id);
    assert_eq!(found.task_id, "task-001".to_string());
    assert_eq!(found.from_id, "user-001".to_string());
    assert_eq!(found.to_id, "".to_string());
    assert_eq!(found.role, MessageRole::User);
    assert_eq!(found.message_type, MessageType::Text);
    assert_eq!(found.content, "你好，这是一条测试消息".to_string());
    assert_eq!(found.meta_json, "".to_string());
    assert_eq!(found.created_by, "test-user".to_string());
}

/// 测试按任务 ID 列表查询
#[sqlx::test]
async fn test_list_by_task_id(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 插入第一条消息
    let msg0 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "你好，这是一条测试消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg0).await.unwrap();

    // 插入更多消息
    let msg1 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "第一条消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let msg2 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "ai-agent-001".to_string(),
        "user-001".to_string(),
        MessageRole::Agent,
        MessageType::Text,
        "第二条消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let msg3 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-002".to_string(), // 不同任务
        "user-002".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "另一个任务的消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg1).await.unwrap();
    // 确保时间顺序
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    message_dao.insert(ctx.clone(), &msg2).await.unwrap();
    message_dao.insert(ctx.clone(), &msg3).await.unwrap();

    // 查询 task-001 的消息（已经有一条，总共 3 条）
    let list = message_dao.list_by_task_id(ctx.clone(), "task-001", None).await.unwrap();
    assert_eq!(list.len(), 3);
    // 按 created_at 升序排列
    assert_eq!(list[0].content, "你好，这是一条测试消息".to_string());
    assert_eq!(list[1].content, "第一条消息".to_string());
    assert_eq!(list[2].content, "第二条消息".to_string());
}

/// 测试分页查询
#[sqlx::test]
async fn test_list_by_task_id_with_limit(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 插入三条消息
    for i in 0..3 {
        let msg = MessagePo::new(
            Uuid::now_v7().to_string(),
            "task-001".to_string(),
            "user-001".to_string(),
            "".to_string(),
            MessageRole::User,
            MessageType::Text,
            format!("消息{}", i),
            "".to_string(),
            "test-user".to_string(),
        );
        message_dao.insert(ctx.clone(), &msg).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }

    let list = message_dao.list_by_task_id(ctx.clone(), "task-001", Some(2)).await.unwrap();
    assert_eq!(list.len(), 2);
}

/// 测试按 from_id 查询
#[sqlx::test]
async fn test_list_by_from_id(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let msg1 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "用户发送".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let msg2 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "ai-agent-001".to_string(),
        "user-001".to_string(),
        MessageRole::Agent,
        MessageType::Text,
        "AI回复".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let msg3 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-002".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "另一个任务".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg1).await.unwrap();
    message_dao.insert(ctx.clone(), &msg2).await.unwrap();
    message_dao.insert(ctx.clone(), &msg3).await.unwrap();

    let list = message_dao.list_by_from_id(ctx.clone(), "user-001", None).await.unwrap();
    assert_eq!(list.len(), 2); // msg1 + msg3
}

/// 测试按 to_id 查询
#[sqlx::test]
async fn test_list_by_to_id(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let msg1 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "用户发送给AI".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let msg2 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "ai-agent-001".to_string(),
        "user-001".to_string(),
        MessageRole::Agent,
        MessageType::Text,
        "AI回复给用户".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg1).await.unwrap();
    message_dao.insert(ctx.clone(), &msg2).await.unwrap();

    let list = message_dao.list_by_to_id(ctx.clone(), "user-001", None).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].content, "AI回复给用户".to_string());
}

/// 测试统计任务消息数量
#[sqlx::test]
async fn test_count_by_task_id(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // task-001: 3 条消息
    for _ in 0..3 {
        let msg = MessagePo::new(
            Uuid::now_v7().to_string(),
            "task-001".to_string(),
            "user-001".to_string(),
            "".to_string(),
            MessageRole::User,
            MessageType::Text,
            "test".to_string(),
            "".to_string(),
            "test-user".to_string(),
        );
        message_dao.insert(ctx.clone(), &msg).await.unwrap();
    }
    // task-002: 1 条消息
    let msg = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-002".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "test".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg).await.unwrap();

    let count = message_dao.count_by_task_id(ctx.clone(), "task-001").await.unwrap();
    assert_eq!(count, 3);
    let count = message_dao.count_by_task_id(ctx.clone(), "task-002").await.unwrap();
    assert_eq!(count, 1);
    let count = message_dao.count_by_task_id(ctx.clone(), "task-not-exists").await.unwrap();
    assert_eq!(count, 0);
}

/// 测试删除消息（软删除，Recalled 状态）
#[sqlx::test]
async fn test_delete_message(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let msg_to_delete = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-delete".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "要删除的消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg_to_delete).await.unwrap();

    // 删除前能找到
    let found = message_dao.find_by_id(ctx.clone(), msg_to_delete.id.as_str()).await.unwrap();
    assert!(found.is_some());

    // 删除（软删除，status 设为 0）
    let result = message_dao.delete(ctx.clone(), msg_to_delete.id.as_str()).await;
    assert!(result.is_ok());

    // 删除后找不到（已过滤）
    let found = message_dao.find_by_id(ctx.clone(), msg_to_delete.id.as_str()).await.unwrap();
    assert!(found.is_none());
}

/// 测试批量删除任务下所有消息（软删除）
#[sqlx::test]
async fn test_delete_by_task_id(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 插入 3 条消息到 task-001
    for i in 0..3 {
        let msg = MessagePo::new(
            Uuid::now_v7().to_string(),
            "task-001".to_string(),
            "user-001".to_string(),
            "".to_string(),
            MessageRole::User,
            MessageType::Text,
            format!("消息{}", i),
            "".to_string(),
            "test-user".to_string(),
        );
        message_dao.insert(ctx.clone(), &msg).await.unwrap();
    }

    // 删除前计数 3
    let count = message_dao.count_by_task_id(ctx.clone(), "task-001").await.unwrap();
    assert_eq!(count, 3);

    // 批量软删除
    let result = message_dao.delete_by_task_id(ctx.clone(), "task-001").await;
    assert!(result.is_ok());

    // 删除后计数 0（全部已撤回，过滤掉）
    let count = message_dao.count_by_task_id(ctx.clone(), "task-001").await.unwrap();
    assert_eq!(count, 0);
}

/// 测试更新消息状态
#[sqlx::test]
async fn test_update_status(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let msg = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "测试消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg).await.unwrap();

    // 更新状态为 Processed
    let result = message_dao.update_status(ctx.clone(), msg.id.as_str(), MessageStatus::Processed).await;
    assert!(result.is_ok());

    let found = message_dao.find_by_id(ctx.clone(), msg.id.as_str()).await.unwrap().unwrap();
    assert_eq!(found.status, MessageStatus::Processed);
}

/// 测试图片消息带元数据
#[sqlx::test]
async fn test_image_message_with_metadata(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 图片消息，content 存文件路径，meta_json 存宽高等信息
    let meta = serde_json::json!({
        "width": 1920,
        "height": 1080,
        "size_bytes": 1024000,
        "mime_type": "image/png"
    }).to_string();

    let msg_image = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Image,
        "20260410/abc123.png".to_string(),
        meta.clone(),
        "test-user".to_string(),
    );

    message_dao.insert(ctx.clone(), &msg_image).await.unwrap();
    let found = message_dao.find_by_id(ctx.clone(), msg_image.id.as_str()).await.unwrap().unwrap();

    assert_eq!(found.message_type, MessageType::Image);
    assert_eq!(found.content, "20260410/abc123.png".to_string());
    assert_eq!(found.meta_json, meta);
}

/// 测试按状态列表查询（用于事件总线恢复）
#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 插入不同状态的消息
    let mut ids = Vec::new();
    for (status, _) in [
        (MessageStatus::Pending, "pending"),
        (MessageStatus::Processing, "processing"),
        (MessageStatus::Processed, "processed"),
    ] {
        let msg = MessagePo::new(
            Uuid::now_v7().to_string(),
            "task-001".to_string(),
            "user-001".to_string(),
            "".to_string(),
            MessageRole::User,
            MessageType::Text,
            "test".to_string(),
            "".to_string(),
            "test-user".to_string(),
        );
        message_dao.insert(ctx.clone(), &msg).await.unwrap();
        // 更新状态
        message_dao.update_status(ctx.clone(), msg.id.as_str(), status).await.unwrap();
        ids.push(msg.id);
    }

    // 查询 Pending + Processing
    let list = message_dao.list_by_status(ctx.clone(), vec![MessageStatus::Pending, MessageStatus::Processing], None).await.unwrap();
    // 应该找到 2 条
    assert_eq!(list.len(), 2);
}
