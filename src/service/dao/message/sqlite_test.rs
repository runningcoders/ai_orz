//! Message DAO SQLite 单元测试

use crate::models::message::MessagePo;
use crate::pkg::storage::Storage;
use common::enums::{MessageRole, MessageType, MessageStatus};
use crate::pkg::RequestContext;
use crate::service::dao::message::{MessageDao, MessageDaoTrait};
use uuid::Uuid;

/// 测试所有 Message DAO 功能
#[tokio::test]
async fn test_all_message_dao_functions() {
    // 使用随机文件名，避免冲突 → 每个测试独立数据库，彻底隔离
    let random_name = format!("/tmp/ai_orz_test_message_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);

    // Storage 自动运行迁移，创建所有表
    let storage = Storage::new(&random_name).await.expect("Failed to create storage");
    let pool = storage.pool();

    let ctx = RequestContext::new(Some("test-user".to_string()), None);
    let message_dao = MessageDao::new(pool.clone());

    // 测试 1: 插入消息并查询
    let msg = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        MessageStatus::Normal,
        "你好，这是一条测试消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = message_dao.insert(ctx.clone(), &msg).await;
    assert!(result.is_ok());

    let found = message_dao.find_by_id(ctx.clone(), msg.id.as_str()).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, Some(msg.id));
    assert_eq!(found.task_id, Some("task-001".to_string()));
    assert_eq!(found.from_id, Some("user-001".to_string()));
    assert_eq!(found.to_id, Some("".to_string()));
    assert_eq!(found.role, MessageRole::User);
    assert_eq!(found.message_type, MessageType::Text);
    assert_eq!(found.content, Some("你好，这是一条测试消息".to_string()));
    assert_eq!(found.meta_json, Some("".to_string()));

    // 测试 2: 按 task_id 列表查询
    let msg1 = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        MessageStatus::Normal,
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
        MessageStatus::Normal,
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
        MessageStatus::Normal,
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
    assert_eq!(list[0].content.clone(), Some("你好，这是一条测试消息".to_string()));
    assert_eq!(list[1].content.clone(), Some("第一条消息".to_string()));
    assert_eq!(list[2].content.clone(), Some("第二条消息".to_string()));

    // 测试 3: 分页查询
    let list = message_dao.list_by_task_id(ctx.clone(), "task-001", Some(2)).await.unwrap();
    assert_eq!(list.len(), 2);

    // 测试 4: 按 from_id 查询
    let list = message_dao.list_by_from_id(ctx.clone(), "user-001", None).await.unwrap();
    // msg + msg1 + msg3 都是 from user-001/002
    assert_eq!(list.len(), 2);

    // 测试 5: 按 to_id 查询
    let list = message_dao.list_by_to_id(ctx.clone(), "user-001", None).await.unwrap();
    assert_eq!(list.len(), 1); // msg2
    assert_eq!(list[0].content.clone(), Some("第二条消息".to_string()));

    // 测试 6: 统计任务消息数量
    let count = message_dao.count_by_task_id(ctx.clone(), "task-001").await.unwrap();
    assert_eq!(count, 3);
    let count = message_dao.count_by_task_id(ctx.clone(), "task-002").await.unwrap();
    assert_eq!(count, 1);
    let count = message_dao.count_by_task_id(ctx.clone(), "task-not-exists").await.unwrap();
    assert_eq!(count, 0);

    // 测试 7: 删除消息
    let msg_to_delete = MessagePo::new(
        Uuid::now_v7().to_string(),
        "task-delete".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        MessageStatus::Normal,
        "要删除的消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg_to_delete).await.unwrap();

    // 删除前能找到
    let found = message_dao.find_by_id(ctx.clone(), msg_to_delete.id.as_str()).await.unwrap();
    assert!(found.is_some());

    // 删除
    let result = message_dao.delete(ctx.clone(), msg_to_delete.id.as_str()).await;
    assert!(result.is_ok());

    // 删除后找不到
    let found = message_dao.find_by_id(ctx.clone(), msg_to_delete.id.as_str()).await.unwrap();
    assert!(found.is_none());

    // 测试 8: 图片消息带元数据
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
        MessageStatus::Normal,
        "20260410/abc123.png".to_string(),
        meta.clone(),
        "test-user".to_string(),
    );

    message_dao.insert(ctx.clone(), &msg_image).await.unwrap();
    let found = message_dao.find_by_id(ctx.clone(), msg_image.id.as_str()).await.unwrap().unwrap();

    assert_eq!(found.message_type, MessageType::Image);
    assert_eq!(found.content, Some("20260410/abc123.png".to_string()));
    assert_eq!(found.meta_json, Some(meta));
}
