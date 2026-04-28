//! Tool DAO SQLite 单元测试

use sqlx::SqlitePool;
use crate::models::tool::{ToolPo, Tool};
use crate::pkg::request_context::RequestContext;
use common::enums::{ToolProtocol, ToolStatus};
use crate::service::dao::tool::ToolDao;
use crate::service::dao::tool::sqlite::{dao, init as dao_init};

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

#[sqlx::test]
async fn test_create_and_get_tool_full(pool: SqlitePool) {
    dao_init();
    crate::pkg::tool_registry::init();
    crate::pkg::tool_registry::init();

    let ctx = RequestContext::new_simple("admin", pool);
    let tool_dao = dao();

    // ========== 测试: 创建工具并查询完整实体
    let tool_po = ToolPo::new(
        "".to_string(), // id 自动生成
        "test-tool".to_string(),
        "A test tool".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("admin".to_string()),
    );
    let result = tool_dao.create_tool(&ctx.clone(), &tool_po).await;
    assert!(result.is_ok());

    // 验证插入成功
    let found = tool_dao.get_by_id(&ctx.clone(), tool_po.id.clone()).await.unwrap();
    assert!(found.is_some());
    let found_po = found.unwrap();
    assert_eq!(found_po.name, "test-tool".to_string());
}



#[sqlx::test]
async fn test_add_tool_to_agent_and_list(pool: SqlitePool) {
    dao_init();
    crate::pkg::tool_registry::init();

    let ctx = RequestContext::new_simple("admin", pool);
    let tool_dao = dao();

    // 创建两个工具
    let tool1 = ToolPo::new(
        "tool-1".to_string(),
        "tool-1".to_string(),
        "Tool 1".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("admin".to_string()),
    );
    let tool2 = ToolPo::new(
        "tool-2".to_string(),
        "tool-2".to_string(),
        "Tool 2".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("admin".to_string()),
    );
    let _ = tool_dao.create_tool(&ctx.clone(), &tool1).await;
    let _ = tool_dao.create_tool(&ctx.clone(), &tool2).await;

    // 绑定到 agent
    let agent_id = "test-agent-1";
    let _ = tool_dao.add_tool_to_agent(&ctx.clone(), agent_id, &tool1.id, Some("test-user".to_string())).await;
    let _ = tool_dao.add_tool_to_agent(&ctx.clone(), agent_id, &tool2.id, Some("test-user".to_string())).await;

    // 测试 list_tools_for_agent
    let list = tool_dao.list_tools_for_agent(&ctx.clone(), agent_id).await.unwrap();
    assert_eq!(list.len(), 2);
    let ids: Vec<String> = list.iter().map(|t| t.id.clone()).collect();
    assert!(ids.contains(&"tool-1".to_string()));
    assert!(ids.contains(&"tool-2".to_string()));
}

#[sqlx::test]
async fn test_remove_tool_from_agent(pool: SqlitePool) {
    dao_init();
    crate::pkg::tool_registry::init();

    let ctx = RequestContext::new_simple("admin", pool);
    let tool_dao = dao();

    // 创建工具并绑定
    let tool = ToolPo::new(
        "tool-to-remove".to_string(),
        "tool-to-remove".to_string(),
        "To remove".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("admin".to_string()),
    );
    let agent_id = "test-agent-2";
    let _ = tool_dao.create_tool(&ctx.clone(), &tool).await;
    let _ = tool_dao.add_tool_to_agent(&ctx.clone(), agent_id, &tool.id, Some("test-user".to_string())).await;

    // 验证绑定成功
    let list = tool_dao.list_tools_for_agent(&ctx.clone(), agent_id).await.unwrap();
    assert_eq!(list.len(), 1);

    // 解绑
    let result = tool_dao.remove_tool_from_agent(&ctx.clone(), agent_id, &tool.id).await;
    assert!(result.is_ok());

    // 验证解绑成功
    let list_after = tool_dao.list_tools_for_agent(&ctx, agent_id).await.unwrap();
    assert!(list_after.is_empty());
}

#[sqlx::test]
async fn test_list_enabled(pool: SqlitePool) {
    dao_init();
    crate::pkg::tool_registry::init();

    let ctx = RequestContext::new_simple("admin", pool);
    let tool_dao = dao();

    // 创建一个启用，一个禁用
    let mut enabled = ToolPo::new(
        "enabled".to_string(),
        "enabled".to_string(),
        "Enabled tool".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("admin".to_string()),
    );
    let mut disabled = ToolPo::new(
        "disabled".to_string(),
        "disabled".to_string(),
        "Disabled tool".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("admin".to_string()),
    );
    disabled.status = ToolStatus::Disabled;

    let _ = tool_dao.create_tool(&ctx.clone(), &enabled).await;
    let _ = tool_dao.create_tool(&ctx.clone(), &disabled).await;

    let list = tool_dao.list_enabled(&ctx).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "enabled".to_string());
}

#[sqlx::test]
async fn test_get_by_name(pool: SqlitePool) {
    dao_init();
    crate::pkg::tool_registry::init();

    let ctx = RequestContext::new_simple("admin", pool);
    let tool_dao = dao();

    let tool = ToolPo::new(
        "".to_string(),
        "my-unique-name".to_string(),
        "Test".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("admin".to_string()),
    );
    let _ = tool_dao.create_tool(&ctx.clone(), &tool).await;

    let found = tool_dao.get_by_name(&ctx, "my-unique-name").await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "my-unique-name");

    let not_found = tool_dao.get_by_name(&ctx, "not-exists").await.unwrap();
    assert!(not_found.is_none());
}

#[sqlx::test]
async fn test_update_tool(pool: SqlitePool) {
    dao_init();
    crate::pkg::tool_registry::init();

    let ctx = RequestContext::new_simple("admin", pool);
    let tool_dao = dao();

    let mut tool = ToolPo::new(
        "".to_string(),
        "original-name".to_string(),
        "Original description".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("creator".to_string()),
    );
    let _ = tool_dao.create_tool(&ctx.clone(), &tool).await;

    // 查询并修改
    let found = tool_dao.get_by_id(&ctx.clone(), tool.id.clone()).await.unwrap().unwrap();
    let mut updated = found;
    updated.name = "updated-name".to_string();
    updated.description = "Updated description".to_string();
    updated.touch(Some("editor".to_string()));

    let result = tool_dao.update_tool(&ctx.clone(), &updated).await;
    assert!(result.is_ok());

    // 验证修改
    let found_after = tool_dao.get_by_id(&ctx, updated.id.clone()).await.unwrap().unwrap();
    assert_eq!(found_after.name, "updated-name");
    assert_eq!(found_after.description, "Updated description");
    assert_eq!(found_after.updated_by, Some("editor".to_string()));
}

#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    dao_init();
    crate::pkg::tool_registry::init();

    let ctx = RequestContext::new_simple("admin", pool);
    let tool_dao = dao();

    let found = tool_dao.get_by_id(&ctx, "not-exist-id".to_string()).await.unwrap();
    assert!(found.is_none());
}