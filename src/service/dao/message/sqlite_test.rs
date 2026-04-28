//! Message DAO SQLite 单元测试

use crate::error::Result;
use crate::models::message::{MessagePo, ToolCallMessage};
use crate::models::file::FileMeta;
use common::enums::{MessageRole, MessageType, MessageStatus, FileType};
use crate::pkg::RequestContext;
use crate::service::dao::message::{self, MessageDao};
use uuid::Uuid;
use sqlx::SqlitePool;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 测试插入消息和按 ID 查询
#[sqlx::test(migrations = "./migrations")]
async fn test_insert_and_find_by_id(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    let msg = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "你好，这是一条测试消息".to_string(),
        None,
        empty_file_meta,
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg).await?;

    let found: Option<MessagePo> = message_dao.find_by_id(ctx.clone(), msg.id.as_str()).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, msg.id);
    assert_eq!(found.task_id, Some("task-001".to_string()));
    assert_eq!(found.task_id, Some("task-001".to_string()));
    assert_eq!(found.from_id, "user-001".to_string());
    assert_eq!(found.to_id, "".to_string());
    assert_eq!(found.from_role, MessageRole::User);
    assert_eq!(found.message_type, MessageType::Text);
    assert_eq!(found.content, "你好，这是一条测试消息".to_string());
    assert_eq!(found.created_by, "test-user".to_string());

    Ok(())
}

/// 测试按任务 ID 列表查询
#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_task_id(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    // 插入第一条消息
    let msg0 = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "你好，这是一条测试消息".to_string(),
        None,
        empty_file_meta.clone(),
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg0).await?;

    // 插入更多消息
    let msg1 = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "第一条消息".to_string(),
        None,
        empty_file_meta.clone(),
        "test-user".to_string(),
    );
    let msg2 = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "ai-agent-001".to_string(),
        "user-001".to_string(),
        MessageRole::Agent,
        MessageRole::User,
        MessageType::Text,
        "第二条消息".to_string(),
        None,
        empty_file_meta.clone(),
        "test-user".to_string(),
    );
    let msg3 = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-002".to_string()), // 不同任务
        "user-002".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "另一个任务的消息".to_string(),
        None,
        empty_file_meta,
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg1).await?;
    // 确保时间顺序
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    message_dao.insert(ctx.clone(), &msg2).await?;
    message_dao.insert(ctx.clone(), &msg3).await?;

    // 查询 task-001 的消息（已经有一条，总共 3 条）
    let list = message_dao.list_by_task_id(ctx.clone(), "task-001", None).await?;
    assert_eq!(list.len(), 3);
    // 按 created_at 升序排列
    assert_eq!(list[0].content, "你好，这是一条测试消息".to_string());
    assert_eq!(list[1].content, "第一条消息".to_string());
    assert_eq!(list[2].content, "第二条消息".to_string());

    Ok(())
}

/// 测试分页查询
#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_task_id_with_limit(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    // 插入三条消息
    for i in 0..3 {
        let msg = MessagePo::new(
            Uuid::now_v7().to_string(),
            None, // project_id (new parameter)
            Some("task-001".to_string()),
            "user-001".to_string(),
            "".to_string(),
            MessageRole::User,
        MessageRole::Agent,
            MessageType::Text,
            format!("消息{}", i),
            None,
            empty_file_meta.clone(),
            "test-user".to_string(),
        );
        message_dao.insert(ctx.clone(), &msg).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }

    let list = message_dao.list_by_task_id(ctx.clone(), "task-001", Some(2)).await?;
    assert_eq!(list.len(), 2);

    Ok(())
}

/// 测试按 from_id 查询
#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_from_id(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    let msg1 = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "用户发送".to_string(),
        None,
        empty_file_meta.clone(),
        "test-user".to_string(),
    );
    let msg2 = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "ai-agent-001".to_string(),
        "user-001".to_string(),
        MessageRole::Agent,
        MessageRole::User,
        MessageType::Text,
        "AI回复".to_string(),
        None,
        empty_file_meta.clone(),
        "test-user".to_string(),
    );
    let msg3 = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-002".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "另一个任务".to_string(),
        None,
        empty_file_meta,
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg1).await?;
    message_dao.insert(ctx.clone(), &msg2).await?;
    message_dao.insert(ctx.clone(), &msg3).await?;

    let list = message_dao.list_by_from_id(ctx.clone(), "user-001", None).await?;
    assert_eq!(list.len(), 2); // msg1 + msg3

    Ok(())
}

/// 测试按 to_id 查询
#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_to_id(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    let msg1 = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "用户发送给AI".to_string(),
        None,
        empty_file_meta.clone(),
        "test-user".to_string(),
    );
    let msg2 = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "ai-agent-001".to_string(),
        "user-001".to_string(),
        MessageRole::Agent,
        MessageRole::User,
        MessageType::Text,
        "AI回复给用户".to_string(),
        None,
        empty_file_meta,
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg1).await?;
    message_dao.insert(ctx.clone(), &msg2).await?;

    let list = message_dao.list_by_to_id(ctx.clone(), "user-001", None).await?;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].content, "AI回复给用户".to_string());

    Ok(())
}

/// 测试统计任务消息数量
#[sqlx::test(migrations = "./migrations")]
async fn test_count_by_task_id(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    // task-001: 3 条消息
    for _ in 0..3 {
        let msg = MessagePo::new(
            Uuid::now_v7().to_string(),
            None, // project_id (new parameter)
            Some("task-001".to_string()),
            "user-001".to_string(),
            "".to_string(),
            MessageRole::User,
        MessageRole::Agent,
            MessageType::Text,
            "test".to_string(),
            None,
            empty_file_meta.clone(),
            "test-user".to_string(),
        );
        message_dao.insert(ctx.clone(), &msg).await?;
    }
    // task-002: 1 条消息
    let msg = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-002".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "test".to_string(),
        None,
        empty_file_meta,
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg).await?;

    let count = message_dao.count_by_task_id(ctx.clone(), "task-001").await?;
    assert_eq!(count, 3);
    let count = message_dao.count_by_task_id(ctx.clone(), "task-002").await?;
    assert_eq!(count, 1);
    let count = message_dao.count_by_task_id(ctx.clone(), "task-not-exists").await?;
    assert_eq!(count, 0);

    Ok(())
}

/// 测试删除消息（软删除，Recalled 状态）
#[sqlx::test(migrations = "./migrations")]
async fn test_delete_message(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    let msg_to_delete = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-delete".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "要删除的消息".to_string(),
        None,
        empty_file_meta,
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg_to_delete).await?;

    // 删除前能找到
    let found = message_dao.find_by_id(ctx.clone(), msg_to_delete.id.as_str()).await?;
    assert!(found.is_some());

    // 删除（软删除，status 设为 0）
    let result = message_dao.delete(ctx.clone(), msg_to_delete.id.as_str()).await;
    assert!(result.is_ok());

    // 删除后找不到（已过滤）
    let found = message_dao.find_by_id(ctx.clone(), msg_to_delete.id.as_str()).await?;
    assert!(found.is_none());

    Ok(())
}

/// 测试批量删除任务下所有消息（软删除）
#[sqlx::test(migrations = "./migrations")]
async fn test_delete_by_task_id(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    // 插入 3 条消息到 task-001
    for i in 0..3 {
        let msg = MessagePo::new(
            Uuid::now_v7().to_string(),
            None, // project_id (new parameter)
            Some("task-001".to_string()),
            "user-001".to_string(),
            "".to_string(),
            MessageRole::User,
        MessageRole::Agent,
            MessageType::Text,
            format!("消息{}", i),
            None,
            empty_file_meta.clone(),
            "test-user".to_string(),
        );
        message_dao.insert(ctx.clone(), &msg).await?;
    }

    // 删除前计数 3
    let count = message_dao.count_by_task_id(ctx.clone(), "task-001").await?;
    assert_eq!(count, 3);

    // 批量软删除
    let result = message_dao.delete_by_task_id(ctx.clone(), "task-001").await;
    assert!(result.is_ok());

    // 删除后计数 0（全部已撤回，过滤掉）
    let count = message_dao.count_by_task_id(ctx.clone(), "task-001").await?;
    assert_eq!(count, 0);

    Ok(())
}

/// 测试更新消息状态
#[sqlx::test(migrations = "./migrations")]
async fn test_update_status(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    let msg = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "测试消息".to_string(),
        None,
        empty_file_meta,
        "test-user".to_string(),
    );
    message_dao.insert(ctx.clone(), &msg).await?;

    // 更新状态为 Processed
    let result = message_dao.update_status(ctx.clone(), msg.id.as_str(), MessageStatus::Processed).await;
    assert!(result.is_ok());

    let found = message_dao.find_by_id(ctx.clone(), msg.id.as_str()).await?.unwrap();
    assert_eq!(found.status, MessageStatus::Processed);

    Ok(())
}

/// 测试图片消息带元数据
#[sqlx::test(migrations = "./migrations")]
async fn test_image_message_with_metadata(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 图片消息，content 存文件路径，file_meta 存宽高等信息
    let file_meta = FileMeta::new(
        "20260410/abc123.png".to_string(),
        "image/png".to_string(),
        1024000,
    );

    let msg_image = MessagePo::new(
        Uuid::now_v7().to_string(),
        None, // project_id (new parameter)
        Some("task-001".to_string()),
        "user-001".to_string(),
        "".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Image,
        "20260410/abc123.png".to_string(),
        Some(FileType::Image),
        file_meta,
        "test-user".to_string(),
    );

    message_dao.insert(ctx.clone(), &msg_image).await?;
    let found = message_dao.find_by_id(ctx.clone(), msg_image.id.as_str()).await?.unwrap();

    assert_eq!(found.message_type, MessageType::Image);
    assert_eq!(found.content, "20260410/abc123.png".to_string());
    assert_eq!(found.file_type, Some(FileType::Image));
    assert_eq!(found.file_meta.0.file_path, "20260410/abc123.png");
    assert_eq!(found.file_meta.0.mime_type, "image/png");
    assert_eq!(found.file_meta.0.file_size, 1024000);

    Ok(())
}

/// 测试按状态列表查询（用于事件总线恢复）
#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_status(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    let empty_file_meta = FileMeta::new(
        "".to_string(),
        "".to_string(),
        0,
    );

    // 插入不同状态的消息
    let mut ids = Vec::new();
    for (status, _) in [
        (MessageStatus::Pending, "pending"),
        (MessageStatus::Processing, "processing"),
        (MessageStatus::Processed, "processed"),
    ] {
        let msg = MessagePo::new(
            Uuid::now_v7().to_string(),
            None, // project_id (new parameter)
            Some("task-001".to_string()),
            "user-001".to_string(),
            "".to_string(),
            MessageRole::User,
        MessageRole::Agent,
            MessageType::Text,
            "test".to_string(),
            None,
            empty_file_meta.clone(),
            "test-user".to_string(),
        );
        message_dao.insert(ctx.clone(), &msg).await?;
        // 更新状态
        message_dao.update_status(ctx.clone(), msg.id.as_str(), status).await?;
        ids.push(msg.id);
    }

    // 查询 Pending + Processing
    let list = message_dao.list_by_status(ctx.clone(), vec![MessageStatus::Pending, MessageStatus::Processing], None).await?;
    // 应该找到 2 条
    assert_eq!(list.len(), 2);

    Ok(())
}
/// 测试创建工具调用请求消息
#[sqlx::test(migrations = "./migrations")]
async fn test_create_tool_call_request(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 创建工具调用请求
    let args = serde_json::json!({
        "query": "搜索关键词",
        "limit": 10
    });

    let req = ToolCallMessage::new_request(
        "test-request-001".to_string(),
        "tool-search-web".to_string(),
        "Search Web".to_string(),
        Some("project-001".to_string()),
        Some("task-001".to_string()),
        "agent-001".to_string(),   // from_id (发起调用的 Agent)
        "executor-agent".to_string(), // to_id (执行工具的 Agent)
        args.clone(),
    );

    let message = message_dao.create_tool_call_request(
        ctx.clone(),
        req,
    ).await?;

    // 查询验证
    let found = message_dao.find_by_id(ctx.clone(), message.id.as_str()).await?;
    assert!(found.is_some());
    let found = found.unwrap();

    // 验证字段
    assert_eq!(found.project_id, Some("project-001".to_string()));
    assert_eq!(found.task_id, Some("task-001".to_string()));
    assert_eq!(found.from_id, "agent-001".to_string());
    assert_eq!(found.to_id, "executor-agent".to_string());
    assert_eq!(found.from_role, MessageRole::Agent);
    assert_eq!(found.message_type, MessageType::ToolCallRequest);
    assert_eq!(found.status, MessageStatus::Pending); // 默认 Pending

    // 验证 content 反序列化后得到正确的 ToolCallMessage
    let parsed: ToolCallMessage = serde_json::from_str(&found.content)?;
    assert_eq!(parsed.args, Some(args));
    assert_eq!(parsed.tool_id, "tool-search-web");
    assert_eq!(parsed.tool_name, "Search Web");
    assert_eq!(parsed.project_id, Some("project-001".to_string()));
    assert_eq!(parsed.task_id, Some("task-001".to_string()));

    Ok(())
}

/// 测试创建工具调用结果消息
#[sqlx::test(migrations = "./migrations")]
async fn test_create_tool_call_result(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 1. 先创建一个工具调用请求（结果需要关联它）
    let args = serde_json::json!({"query": "test"});

    let request_msg = ToolCallMessage::new_request(
        "test-request-001".to_string(),
        "tool-search".to_string(),
        "Search".to_string(),
        Some("project-001".to_string()),
        Some("task-001".to_string()),
        "agent-001".to_string(),
        "executor-agent".to_string(),
        args,
    );

    let request = message_dao.create_tool_call_request(
        ctx.clone(),
        request_msg.clone(),
    ).await?;

    // 2. 创建工具调用成功结果
    let result_data = serde_json::json!([
        {"title": "Result 1", "url": "https://example.com"},
        {"title": "Result 2", "url": "https://example.org"},
    ]);

    // 从原始请求创建成功结果，自动继承上下文
    let result_msg = request_msg.new_success_result(result_data.clone(), None);

    let result_msg = message_dao.create_tool_call_result(
        ctx.clone(),
        result_msg,
    ).await?;

    // 3. 查询验证
    let found = message_dao.find_by_id(ctx.clone(), result_msg.id.as_str()).await?;
    assert!(found.is_some());
    let found = found.unwrap();

    assert_eq!(found.project_id, Some("project-001".to_string()));
    assert_eq!(found.task_id, Some("task-001".to_string()));
    assert_eq!(found.from_id, "executor-agent".to_string());
    assert_eq!(found.to_id, "agent-001".to_string());
    assert_eq!(found.from_role, MessageRole::System);
    assert_eq!(found.message_type, MessageType::ToolCallResult);

    // 解析 content 验证结构
    let parsed: ToolCallMessage = serde_json::from_str(&found.content)?;
    
    assert_eq!(parsed.request_id, "test-request-001");
    assert_eq!(parsed.tool_id, "tool-search".to_string());
    assert_eq!(parsed.is_success, Some(true));
    assert_eq!(parsed.result, Some(result_data));


    Ok(())
}

/// 测试创建工具调用结果消息（失败情况）
#[sqlx::test(migrations = "./migrations")]
async fn test_create_tool_call_result_failed(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 先创建请求
    let args = serde_json::json!({"path": "/nonexistent"});

    let request_msg = ToolCallMessage::new_request(
        "test-request-002".to_string(),
        "tool-read-file".to_string(),
        "Read File".to_string(),
        None, // project_id
        Some("task-001".to_string()),
        "agent-001".to_string(),
        "executor-agent".to_string(),
        args,
    );

    let request = message_dao.create_tool_call_request(
        ctx.clone(),
        request_msg.clone(),
    ).await?;

    // 创建失败结果
    let error_result = serde_json::json!({
        "error": "File not found",
        "error_code": "ENOENT"
    });

    // 从原始请求创建失败结果，自动继承上下文
    let result_msg = request_msg.new_error_result_with_data(error_result, "File not found".to_string());

    let result_msg = message_dao.create_tool_call_result(
        ctx.clone(),
        result_msg,
    ).await?;

    let found = message_dao.find_by_id(ctx.clone(), result_msg.id.as_str()).await?;
    let found = found.unwrap();
    // 解析 content 验证结构
    let parsed: ToolCallMessage = serde_json::from_str(&found.content)?;
    
    assert_eq!(parsed.request_id, "test-request-002");
    assert_eq!(parsed.is_success, Some(false));


    Ok(())
}


/// 测试按 project_id 查询消息列表
#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_project_id(pool: SqlitePool) -> Result<()> {
    crate::service::dao::message::init();
    let message_dao = message::dao();
    let ctx = new_ctx("test-user", pool);

    // 创建多条消息分属不同项目
    // project-1: 3 条消息
    let id1 = Uuid::now_v7().to_string();
    let m1 = MessagePo::new(
        id1,
        Some("project-1".to_string()),
        Some("task-1".to_string()),
        "user-1".to_string(),
        "agent-1".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "Hello project 1".to_string(),
        None,
        FileMeta::default(),
        "test-user".to_string(),
    );
    let _m1 = message_dao.insert(ctx.clone(), &m1).await?;

    let id2 = Uuid::now_v7().to_string();
    let m2 = MessagePo::new(
        id2,
        Some("project-1".to_string()),
        Some("task-1".to_string()),
        "agent-1".to_string(),
        "user-1".to_string(),
        MessageRole::Agent,
        MessageRole::User,
        MessageType::Text,
        "Reply to project 1".to_string(),
        None,
        FileMeta::default(),
        "test-user".to_string(),
    );
    let _m2 = message_dao.insert(ctx.clone(), &m2).await?;

    let id3 = Uuid::now_v7().to_string();
    let m3 = MessagePo::new(
        id3,
        Some("project-1".to_string()),
        Some("task-2".to_string()),
        "user-1".to_string(),
        "agent-1".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "Second message in project 1".to_string(),
        None,
        FileMeta::default(),
        "test-user".to_string(),
    );
    let _m3 = message_dao.insert(ctx.clone(), &m3).await?;

    // project-2: 1 条消息
    let id4 = Uuid::now_v7().to_string();
    let m4 = MessagePo::new(
        id4,
        Some("project-2".to_string()),
        Some("task-3".to_string()),
        "user-2".to_string(),
        "agent-2".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        "Hello project 2".to_string(),
        None,
        FileMeta::default(),
        "test-user".to_string(),
    );
    let _m4 = message_dao.insert(ctx.clone(), &m4).await?;

    // 查询 project-1 应该返回 3 条
    let list = message_dao.list_by_project_id(ctx.clone(), "project-1", None).await?;
    assert_eq!(list.len(), 3);
    // 按创建时间正序排列，最早的在最前面
    assert_eq!(list[0].content, "Hello project 1");
    assert_eq!(list[1].content, "Reply to project 1");
    assert_eq!(list[2].content, "Second message in project 1");

    // 验证所有消息都属于正确的 project_id
    for msg in &list {
        assert_eq!(msg.project_id, Some("project-1".to_string()));
    }

    // 查询 project-2 应该返回 1 条
    let list = message_dao.list_by_project_id(ctx.clone(), "project-2", None).await?;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].content, "Hello project 2");

    // 查询 project-3 应该返回空
    let list = message_dao.list_by_project_id(ctx.clone(), "project-3", None).await?;
    assert!(list.is_empty());

    // 测试 limit 限制
    let list = message_dao.list_by_project_id(ctx.clone(), "project-1", Some(2)).await?;
    assert_eq!(list.len(), 2);

    Ok(())
}
