//! Message DAO 单元测试
//!
//! 使用单个临时数据库文件运行所有测试

use super::*;
use crate::models::message::MessagePo;
use crate::pkg::storage;
use common::constants::{MessageRole, MessageType, RequestContext};
use crate::service::dao::message::sqlite::MessageDaoImpl;

/// 运行所有测试在同一个数据库初始化中，避免 OnceLock 重复初始化问题
#[test]
fn test_all_message_dao_functions() {
    // 准备工作：删除旧的临时数据库，初始化全局存储
    let test_db_path = "/tmp/ai_orz_test_message_all.db".to_string();
    let _ = std::fs::remove_file(&test_db_path);
    // 强制重新初始化（忽略已经初始化的错误，多线程测试可能重复进入）
    match storage::init(&test_db_path) {
        Ok(_) => {},
        Err(_) => {}, // 如果已经初始化，忽略
    }

    let ctx = RequestContext::new(Some("test-user".to_string()), None);
    let dao = MessageDaoImpl::new();

    // ========== 测试 1: 插入消息并查询 ==========
    let msg = MessagePo::new(
        uuid::Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "你好，这是一条测试消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    let result = dao.insert(ctx.clone(), &msg);
    assert!(result.is_ok());

    let found = dao.find_by_id(ctx.clone(), msg.id.as_str()).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, msg.id);
    assert_eq!(found.task_id, "task-001");
    assert_eq!(found.from_id, "user-001");
    assert_eq!(found.to_id, "");
    assert_eq!(found.role, MessageRole::User);
    assert_eq!(found.message_type, MessageType::Text);
    assert_eq!(found.content, "你好，这是一条测试消息");
    assert_eq!(found.meta_json, "");

    // ========== 测试 2: 按 task_id 列表查询 ==========
    let msg1 = MessagePo::new(
        uuid::Uuid::now_v7().to_string(),
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
        uuid::Uuid::now_v7().to_string(),
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
        uuid::Uuid::now_v7().to_string(),
        "task-002".to_string(), // 不同任务
        "user-002".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "另一个任务的消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &msg1).unwrap();
    // 为了确保时间顺序，稍微等一下
    std::thread::sleep(std::time::Duration::from_millis(10));
    dao.insert(ctx.clone(), &msg2).unwrap();
    dao.insert(ctx.clone(), &msg3).unwrap();

    // 查询 task-001 的消息（已经有一条，总共 3 条）
    let list = dao.list_by_task_id(ctx.clone(), "task-001", None).unwrap();
    assert_eq!(list.len(), 3);
    // 按 created_at 升序排列
    assert_eq!(list[0].content, "你好，这是一条测试消息");
    assert_eq!(list[1].content, "第一条消息");
    assert_eq!(list[2].content, "第二条消息");

    // ========== 测试 3: 分页查询 ==========
    let list = dao.list_by_task_id(ctx.clone(), "task-001", Some(2)).unwrap();
    assert_eq!(list.len(), 2);

    // ========== 测试 4: 按 from_id 查询 ==========
    let list = dao.list_by_from_id(ctx.clone(), "user-001", None).unwrap();
    // msg + msg1 + msg3 都是 from user-001/002
    assert_eq!(list.len(), 2);

    // ========== 测试 5: 按 to_id 查询 ==========
    let list = dao.list_by_to_id(ctx.clone(), "user-001", None).unwrap();
    assert_eq!(list.len(), 1); // msg2
    assert_eq!(list[0].content, "第二条消息");

    // ========== 测试 6: 统计任务消息数量 ==========
    let count = dao.count_by_task_id(ctx.clone(), "task-001").unwrap();
    assert_eq!(count, 3);
    let count = dao.count_by_task_id(ctx.clone(), "task-002").unwrap();
    assert_eq!(count, 1);
    let count = dao.count_by_task_id(ctx.clone(), "task-not-exists").unwrap();
    assert_eq!(count, 0);

    // ========== 测试 7: 删除消息 ==========
    let msg_to_delete = MessagePo::new(
        uuid::Uuid::now_v7().to_string(),
        "task-delete".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Text,
        "要删除的消息".to_string(),
        "".to_string(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &msg_to_delete).unwrap();

    // 删除前能找到
    let found = dao.find_by_id(ctx.clone(), msg_to_delete.id.as_str()).unwrap();
    assert!(found.is_some());

    // 删除
    let result = dao.delete(ctx.clone(), msg_to_delete.id.as_str());
    assert!(result.is_ok());

    // 删除后找不到
    let found = dao.find_by_id(ctx.clone(), msg_to_delete.id.as_str()).unwrap();
    assert!(found.is_none());

    // ========== 测试 8: 图片消息带元数据 ==========
    // 图片消息，content 存文件路径，meta_json 存宽高等信息
    let meta = serde_json::json!({
        "width": 1920,
        "height": 1080,
        "size_bytes": 1024000,
        "mime_type": "image/png"
    }).to_string();

    let msg_image = MessagePo::new(
        uuid::Uuid::now_v7().to_string(),
        "task-001".to_string(),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageType::Image,
        "20260410/abc123.png".to_string(),
        meta.clone(),
        "test-user".to_string(),
    );

    dao.insert(ctx.clone(), &msg_image).unwrap();
    let found = dao.find_by_id(ctx, msg_image.id.as_str()).unwrap().unwrap();

    assert_eq!(found.message_type, MessageType::Image);
    assert_eq!(found.content, "20260410/abc123.png");
    assert_eq!(found.meta_json, meta);

    // 所有测试通过!
}
