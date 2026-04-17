//! Tool DAO SQLite 单元测试

use sqlx::SqlitePool;
use common::enums::{ToolProtocol, ToolStatus};
use crate::error::AppError;
use crate::models::tool::ToolPo;
use crate::pkg::RequestContext;
use crate::service::dao::tool::{self};
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 测试插入新工具并按 ID 查询
#[sqlx::test]
async fn test_create_and_get_by_id(pool: SqlitePool) -> Result<(), AppError> {
    tool::init();
    let dao = tool::dao();

    let po = ToolPo::new(
        Uuid::now_v7().to_string(),
        "Test Tool".to_string(),
        "A test tool for unit testing".to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        Some("test-user".to_string()),
    );
    let tool_id = po.id.clone();

    let ctx = new_ctx("test-user", pool.clone());
    dao.create_tool(&ctx, &po).await?;

    let ctx = new_ctx("test-user", pool);
    let found: Option<ToolPo> = dao.get_by_id(&ctx, tool_id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, po.id);
    assert_eq!(found.name, "Test Tool");
    assert_eq!(found.description, "A test tool for unit testing");
    assert_eq!(found.protocol, ToolProtocol::Builtin);
    assert_eq!(found.status, ToolStatus::Enabled);

    Ok(())
}

/// 测试更新工具
#[sqlx::test]
async fn test_update_tool(pool: SqlitePool) -> Result<(), AppError> {
    tool::init();
    let dao = tool::dao();

    let mut po = ToolPo::new(
        Uuid::now_v7().to_string(),
        "Test Update".to_string(),
        "Original description".to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        Some("test-user".to_string()),
    );
    let tool_id = po.id.clone();

    let ctx = new_ctx("test-user", pool.clone());
    dao.create_tool(&ctx, &po).await?;

    po.description = "Updated description".to_string();
    po.name = "Updated Name".to_string();
    po.status = ToolStatus::Disabled;

    let ctx = new_ctx("test-user", pool.clone());
    dao.update_tool(&ctx, &po).await?;

    let ctx = new_ctx("test-user", pool);
    let found: Option<ToolPo> = dao.get_by_id(&ctx, tool_id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "Updated Name");
    assert_eq!(found.description, "Updated description");
    assert_eq!(found.status, ToolStatus::Disabled);

    Ok(())
}

/// 测试按名称查询
#[sqlx::test]
async fn test_get_by_name(pool: SqlitePool) -> Result<(), AppError> {
    tool::init();
    let dao = tool::dao();

    let po = ToolPo::new(
        Uuid::now_v7().to_string(),
        "Get By Name Test".to_string(),
        "".to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        Some("test-user".to_string()),
    );
    let tool_id = po.id.clone();

    let ctx = new_ctx("test-user", pool.clone());
    dao.create_tool(&ctx, &po).await?;

    let ctx = new_ctx("test-user", pool);
    let found: Option<ToolPo> = dao.get_by_name(&ctx, "Get By Name Test").await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, po.id);

    Ok(())
}

/// 测试列出已启用工具
#[sqlx::test]
async fn test_list_enabled(pool: SqlitePool) -> Result<(), AppError> {
    tool::init();
    let dao = tool::dao();

    let po1 = ToolPo::new(
        Uuid::now_v7().to_string(),
        "Enabled Tool 1".to_string(),
        "".to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        Some("test-user".to_string()),
    );
    let id1 = po1.id.clone();

    let mut po2 = ToolPo::new(
        Uuid::now_v7().to_string(),
        "Disabled Tool".to_string(),
        "".to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        Some("test-user".to_string()),
    );
    po2.status = ToolStatus::Disabled;
    let id2 = po2.id.clone();

    let po3 = ToolPo::new(
        Uuid::now_v7().to_string(),
        "Enabled Tool 2".to_string(),
        "".to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        Some("test-user".to_string()),
    );
    let id3 = po3.id.clone();

    let ctx = new_ctx("test-user", pool.clone());
    dao.create_tool(&ctx.clone(), &po1).await?;
    dao.create_tool(&ctx.clone(), &po2).await?;
    dao.create_tool(&ctx, &po3).await?;

    let ctx = new_ctx("test-user", pool);
    let enabled: Vec<ToolPo> = dao.list_enabled(&ctx).await?;
    assert_eq!(enabled.len(), 2);
    assert!(enabled.iter().any(|t| t.id == id1));
    assert!(enabled.iter().any(|t| t.id == id3));
    assert!(!enabled.iter().any(|t| t.id == id2));

    Ok(())
}

/// 测试添加工具到 Agent 和列出 Agent 工具
#[sqlx::test]
async fn test_add_and_list_for_agent(pool: SqlitePool) -> Result<(), AppError> {
    tool::init();
    let dao = tool::dao();

    // 创建两个工具
    let tool1 = ToolPo::new(
        Uuid::now_v7().to_string(),
        "Agent Tool 1".to_string(),
        "".to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        Some("test-user".to_string()),
    );
    let tool1_id = tool1.id.clone();

    let tool2 = ToolPo::new(
        Uuid::now_v7().to_string(),
        "Agent Tool 2".to_string(),
        "".to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        Some("test-user".to_string()),
    );
    let tool2_id = tool2.id.clone();

    let agent_id = "test-agent-123";

    let ctx = new_ctx("test-user", pool.clone());
    dao.create_tool(&ctx.clone(), &tool1).await?;
    dao.create_tool(&ctx.clone(), &tool2).await?;
    dao.add_tool_to_agent(&ctx.clone(), agent_id, &tool1_id, Some("test-user".to_string())).await?;
    dao.add_tool_to_agent(&ctx, agent_id, &tool2_id, Some("test-user".to_string())).await?;

    // 列出 Agent 的工具
    let ctx = new_ctx("test-user", pool);
    let tools: Vec<ToolPo> = dao.list_tools_for_agent(&ctx, agent_id).await?;
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().any(|t| t.id == tool1_id));
    assert!(tools.iter().any(|t| t.id == tool2_id));

    Ok(())
}

/// 测试从 Agent 移除工具
#[sqlx::test]
async fn test_remove_from_agent(pool: SqlitePool) -> Result<(), AppError> {
    tool::init();
    let dao = tool::dao();

    let tool1 = ToolPo::new(
        Uuid::now_v7().to_string(),
        "To Remove Tool".to_string(),
        "".to_string(),
        ToolProtocol::Builtin,
        serde_json::json!({}),
        None,
        Some("test-user".to_string()),
    );
    let tool1_id = tool1.id.clone();

    let agent_id = "test-agent-456";

    let ctx = new_ctx("test-user", pool.clone());
    dao.create_tool(&ctx.clone(), &tool1).await?;
    dao.add_tool_to_agent(&ctx.clone(), agent_id, &tool1_id, Some("test-user".to_string())).await?;

    // 确认添加成功
    let ctx = new_ctx("test-user", pool.clone());
    let before: Vec<ToolPo> = dao.list_tools_for_agent(&ctx, agent_id).await?;
    assert_eq!(before.len(), 1);

    // 移除
    let ctx = new_ctx("test-user", pool.clone());
    dao.remove_tool_from_agent(&ctx, agent_id, &tool1_id).await?;

    // 确认移除成功
    let ctx = new_ctx("test-user", pool);
    let after: Vec<ToolPo> = dao.list_tools_for_agent(&ctx, agent_id).await?;
    assert_eq!(after.len(), 0);

    Ok(())
}
