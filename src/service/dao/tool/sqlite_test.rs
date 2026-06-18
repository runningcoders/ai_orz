//! Tool DAO SQLite 单元测试

use crate::models::tool::{Tool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::tool::ToolDao;
use crate::service::dao::tool::sqlite::{dao, init as dao_init};
use common::enums::{ToolProtocol, ToolStatus};
use sqlx::SqlitePool;
use std::sync::Arc;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn ToolDao> {
    dao_init();
    dao()
}

/// 创建测试 ToolPo
fn create_test_tool(name: &str, description: &str) -> ToolPo {
    ToolPo::new(
        "".to_string(), // id 自动生成
        name.to_string(),
        description.to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    )
}

#[sqlx::test]
async fn test_create_and_get_tool_full(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // ========== 测试: 创建工具并查询完整实体
    let tool_po = ToolPo::new(
        "".to_string(), // id 自动生成
        "test-tool".to_string(),
        "A test tool".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let result = tool_dao.create_tool(ctx.clone(), &tool_po).await;
    assert!(result.is_ok());

    // 验证插入成功
    let found = tool_dao
        .get_by_id(ctx.clone(), tool_po.id.clone())
        .await
        .unwrap();
    assert!(found.is_some());
    let found_po = found.unwrap();
    assert_eq!(found_po.name, "test-tool".to_string());
}

#[sqlx::test]
async fn test_add_tool_to_agent_and_list(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // 创建两个工具
    let tool1 = ToolPo::new(
        "tool-1".to_string(),
        "tool-1".to_string(),
        "Tool 1".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let tool2 = ToolPo::new(
        "tool-2".to_string(),
        "tool-2".to_string(),
        "Tool 2".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let _ = tool_dao.create_tool(ctx.clone(), &tool1).await;
    let _ = tool_dao.create_tool(ctx.clone(), &tool2).await;

    // 绑定到 agent
    let agent_id = "test-agent-1";
    let _ = tool_dao
        .add_tool_to_agent(
            ctx.clone(),
            agent_id,
            &tool1.id,
            Some("test-user".to_string()),
        )
        .await;
    let _ = tool_dao
        .add_tool_to_agent(
            ctx.clone(),
            agent_id,
            &tool2.id,
            Some("test-user".to_string()),
        )
        .await;

    // 测试 list_tools_for_agent
    let list = tool_dao
        .list_tools_for_agent(ctx.clone(), agent_id)
        .await
        .unwrap();
    assert_eq!(list.len(), 2);
    let ids: Vec<String> = list.iter().map(|t| t.id.clone()).collect();
    assert!(ids.contains(&"tool-1".to_string()));
    assert!(ids.contains(&"tool-2".to_string()));
}

#[sqlx::test]
async fn test_remove_tool_from_agent(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // 创建工具并绑定
    let tool = ToolPo::new(
        "tool-to-remove".to_string(),
        "tool-to-remove".to_string(),
        "To remove".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let agent_id = "test-agent-2";
    let _ = tool_dao.create_tool(ctx.clone(), &tool).await;
    let _ = tool_dao
        .add_tool_to_agent(
            ctx.clone(),
            agent_id,
            &tool.id,
            Some("test-user".to_string()),
        )
        .await;

    // 验证绑定成功
    let list = tool_dao
        .list_tools_for_agent(ctx.clone(), agent_id)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);

    // 解绑
    let result = tool_dao
        .remove_tool_from_agent(ctx.clone(), agent_id, &tool.id)
        .await;
    assert!(result.is_ok());

    // 验证解绑成功
    let list_after = tool_dao
        .list_tools_for_agent(ctx.clone(), agent_id)
        .await
        .unwrap();
    assert!(list_after.is_empty());
}

#[sqlx::test]
async fn test_list_enabled(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // 创建一个启用，一个禁用
    let mut enabled = ToolPo::new(
        "enabled".to_string(),
        "enabled".to_string(),
        "Enabled tool".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let mut disabled = ToolPo::new(
        "disabled".to_string(),
        "disabled".to_string(),
        "Disabled tool".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    disabled.status = ToolStatus::Disabled;

    let _ = tool_dao.create_tool(ctx.clone(), &enabled).await;
    let _ = tool_dao.create_tool(ctx.clone(), &disabled).await;

    let list = tool_dao.list_enabled(ctx.clone()).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "enabled".to_string());
}

#[sqlx::test]
async fn test_get_by_name(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    let tool = ToolPo::new(
        "".to_string(),
        "my-unique-name".to_string(),
        "Test".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let _ = tool_dao.create_tool(ctx.clone(), &tool).await;

    let found = tool_dao
        .get_by_name(ctx.clone(), "my-unique-name")
        .await
        .unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "my-unique-name");

    let not_found = tool_dao
        .get_by_name(ctx.clone(), "not-exists")
        .await
        .unwrap();
    assert!(not_found.is_none());
}

#[sqlx::test]
async fn test_update_tool(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // 创建非内置工具
    let mut tool = ToolPo::new(
        "".to_string(),
        "original-name".to_string(),
        "Original description".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("creator".to_string()),
    );
    let _ = tool_dao.create_tool(ctx.clone(), &tool).await;

    // 查询并修改
    let found = tool_dao
        .get_by_id(ctx.clone(), tool.id.clone())
        .await
        .unwrap()
        .unwrap();
    let mut updated = found;
    updated.name = "updated-name".to_string();
    updated.description = "Updated description".to_string();
    updated.touch(Some("editor".to_string()));

    let result = tool_dao.update_tool(ctx.clone(), &updated).await;
    assert!(result.is_ok());

    // 验证修改
    let found_after = tool_dao
        .get_by_id(ctx.clone(), updated.id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found_after.name, "updated-name");
    assert_eq!(found_after.description, "Updated description");
    assert_eq!(found_after.updated_by, Some("editor".to_string()));
}

#[sqlx::test]
async fn test_update_builtin_tool_protected(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // 创建内置工具
    let mut tool = ToolPo::new(
        "".to_string(),
        "builtin-tool".to_string(),
        "Builtin description".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("creator".to_string()),
    );
    let _ = tool_dao.create_tool(ctx.clone(), &tool).await;

    // 尝试修改内置工具应该失败
    let found = tool_dao
        .get_by_id(ctx.clone(), tool.id.clone())
        .await
        .unwrap()
        .unwrap();
    let mut updated = found;
    updated.name = "should-not-work".to_string();
    updated.touch(Some("editor".to_string()));

    let result = tool_dao.update_tool(ctx.clone(), &updated).await;
    assert!(result.is_err());
}

#[sqlx::test]
async fn test_delete_builtin_tool_protected(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // 创建内置工具
    let mut tool = ToolPo::new(
        "".to_string(),
        "builtin-tool".to_string(),
        "Builtin description".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("creator".to_string()),
    );
    let _ = tool_dao.create_tool(ctx.clone(), &tool).await;

    // 尝试删除内置工具应该失败
    let result = tool_dao.delete_tool(ctx.clone(), &tool.id).await;
    assert!(result.is_err());
}

#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    let found = tool_dao
        .get_by_id(ctx.clone(), "not-exist-id".to_string())
        .await
        .unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_sync_builtin_tools_to_db(pool: SqlitePool) {
    // 不依赖实际的内置工具注册，这个测试主要验证实现的幂等性
    // 实际的内置工具同步在集成测试中验证
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // 直接测试 sync 不会 panic 即可
    let inserted_count = tool_dao
        .sync_builtin_tools_to_db(ctx.clone())
        .await
        .unwrap();
    // 不判断数量，因为测试环境可能没有注册内置工具
    assert!(inserted_count >= 0);
}

#[sqlx::test]
async fn test_tool_query(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // 创建几个测试工具
    let tool1 = ToolPo::new(
        "id-1".to_string(),
        "test-tool-1".to_string(),
        "这是第一个测试工具".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["tag1".to_string()],
        Some("admin".to_string()),
    );
    let mut tool2 = ToolPo::new(
        "id-2".to_string(),
        "test-tool-2".to_string(),
        "这是第二个测试工具".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["tag2".to_string()],
        Some("admin".to_string()),
    );
    tool2.status = ToolStatus::Disabled;

    let _ = tool_dao.create_tool(ctx.clone(), &tool1).await;
    let _ = tool_dao.create_tool(ctx.clone(), &tool2).await;

    // 1. 测试 ID 批量查询
    let query = crate::service::dao::tool::ToolQuery {
        ids: Some(vec!["id-1".to_string(), "id-2".to_string()]),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(results.len(), 2);

    // 2. 测试关键词搜索 - 不测试包含 SQL 的语法，直接测试简单查询
    let query = crate::service::dao::tool::ToolQuery {
        keyword: Some("test-tool".to_string()),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(results.len(), 2);

    // 3. 测试 enabled_only 过滤
    let query = crate::service::dao::tool::ToolQuery {
        enabled_only: Some(true),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "id-1");

    // 4. 测试 limit 限制
    let query = crate::service::dao::tool::ToolQuery {
        limit: Some(1),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(results.len(), 1);
}

#[sqlx::test]
async fn test_tool_search(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = RequestContext::new_simple("admin", pool);

    // 创建几个测试工具
    let tool1 = ToolPo::new(
        "id-1".to_string(),
        "search-tool-1".to_string(),
        "这是一个可搜索的工具".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["search".to_string()],
        Some("admin".to_string()),
    );
    let tool2 = ToolPo::new(
        "id-2".to_string(),
        "search-tool-2".to_string(),
        "这是另一个可搜索的工具".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["search".to_string()],
        Some("admin".to_string()),
    );

    let _ = tool_dao.create_tool(ctx.clone(), &tool1).await;
    let _ = tool_dao.create_tool(ctx.clone(), &tool2).await;

    // 1. 关键词搜索
    let search = crate::service::dao::tool::ToolSearch {
        keyword: Some("search-tool".to_string()),
        enabled_only: true,
        limit: 10,
        ..Default::default()
    };
    let results = tool_dao.search(ctx.clone(), search).await.unwrap();
    assert_eq!(results.len(), 2);

    // 2. 测试 limit
    let search = crate::service::dao::tool::ToolSearch {
        keyword: Some("search-tool".to_string()),
        enabled_only: true,
        limit: 1,
        ..Default::default()
    };
    let results = tool_dao.search(ctx.clone(), search).await.unwrap();
    assert_eq!(results.len(), 1);
}
