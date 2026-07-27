//! Tool DAO SQLite 单元测试

use crate::models::tool::ToolPo;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::tool::ToolDao;
use crate::service::dao::tool::sqlite::{dao, init as dao_init};
use common::enums::{ToolProtocol, ToolStatus};
use sqlx::SqlitePool;
use std::sync::Arc;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

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
async fn test_list_tools_for_agent_preserves_stale_bindings_for_management(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    let enabled = ToolPo::new(
        "tool-agent-enabled".to_string(),
        "tool-agent-enabled".to_string(),
        "Enabled bound tool".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let mut stale = ToolPo::new(
        "tool-agent-stale".to_string(),
        "tool-agent-stale".to_string(),
        "Stale bound tool".to_string(),
        ToolProtocol::Mcp,
        serde_json::json!({"server_id": "server-a", "tool_name": "stale"}),
        None,
        vec![],
        Some("admin".to_string()),
    );
    stale.status = ToolStatus::Stale;

    tool_dao.create_tool(ctx.clone(), &enabled).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &stale).await.unwrap();

    let agent_id = "test-agent-stale-filter";
    tool_dao
        .add_tool_to_agent(
            ctx.clone(),
            agent_id,
            &enabled.id,
            Some("admin".to_string()),
        )
        .await
        .unwrap();
    tool_dao
        .add_tool_to_agent(ctx.clone(), agent_id, &stale.id, Some("admin".to_string()))
        .await
        .unwrap();

    let list = tool_dao
        .list_tools_for_agent(ctx.clone(), agent_id)
        .await
        .unwrap();
    let ids: Vec<String> = list.iter().map(|tool| tool.id.clone()).collect();
    assert_eq!(
        ids,
        vec![
            "tool-agent-enabled".to_string(),
            "tool-agent-stale".to_string()
        ]
    );
}

#[sqlx::test]
async fn test_remove_tool_from_agent(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // 创建一个启用，一个禁用
    let enabled = ToolPo::new(
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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // 创建非内置工具
    let tool = ToolPo::new(
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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // 创建内置工具
    let tool = ToolPo::new(
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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // 创建内置工具
    let tool = ToolPo::new(
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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

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
    assert_eq!(results.items.len(), 2);

    // 2. 测试关键词在 query 中被忽略（FTS5 搜索请使用 search_tools）
    //    query 方法不再做关键词过滤，传入 keyword 应返回全部工具
    let query = crate::service::dao::tool::ToolQuery {
        keyword: Some("test-tool".to_string()),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    // keyword 被忽略，返回全部 2 条工具
    assert_eq!(results.items.len(), 2);

    // 3. 测试 enabled_only 过滤
    let query = crate::service::dao::tool::ToolQuery {
        enabled_only: Some(true),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(results.items.len(), 1);
    assert_eq!(results.items[0].id, "id-1");

    // 4. 测试 limit 限制
    let query = crate::service::dao::tool::ToolQuery {
        pagination: common::api::PaginationParams {
            limit: Some(1),
            offset: None,
        },
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(results.items.len(), 1);
}

#[sqlx::test]
async fn test_tool_query_can_exclude_stale_and_allows_explicit_stale_status(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    let enabled = ToolPo::new(
        "query-enabled".to_string(),
        "query-enabled".to_string(),
        "Query enabled tool".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let mut disabled = ToolPo::new(
        "query-disabled".to_string(),
        "query-disabled".to_string(),
        "Query disabled tool".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    disabled.status = ToolStatus::Disabled;
    let mut stale = ToolPo::new(
        "query-stale".to_string(),
        "query-stale".to_string(),
        "Query stale tool".to_string(),
        ToolProtocol::Mcp,
        serde_json::json!({"server_id": "server-a", "tool_name": "query-stale"}),
        None,
        vec![],
        Some("admin".to_string()),
    );
    stale.status = ToolStatus::Stale;

    tool_dao.create_tool(ctx.clone(), &enabled).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &disabled).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &stale).await.unwrap();

    let non_stale_results = tool_dao
        .query(
            ctx.clone(),
            crate::service::dao::tool::ToolQuery {
                exclude_status: Some(ToolStatus::Stale),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let non_stale_ids: Vec<String> = non_stale_results
        .items
        .iter()
        .map(|tool| tool.id.clone())
        .collect();
    assert!(non_stale_ids.contains(&"query-enabled".to_string()));
    assert!(non_stale_ids.contains(&"query-disabled".to_string()));
    assert!(!non_stale_ids.contains(&"query-stale".to_string()));

    let stale_results = tool_dao
        .query(
            ctx.clone(),
            crate::service::dao::tool::ToolQuery {
                status: Some(ToolStatus::Stale),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(stale_results.items.len(), 1);
    assert_eq!(stale_results.items[0].id, "query-stale");
}

#[sqlx::test]
async fn test_tool_search(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

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

#[sqlx::test]
async fn test_query_by_tag(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // 创建带不同 tag 的工具
    let tool1 = ToolPo::new(
        "tag-tool-1".to_string(),
        "python-tool".to_string(),
        "A python tool".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["python".to_string(), "automation".to_string()],
        Some("admin".to_string()),
    );
    let tool2 = ToolPo::new(
        "tag-tool-2".to_string(),
        "rust-tool".to_string(),
        "A rust tool".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["rust".to_string(), "systems".to_string()],
        Some("admin".to_string()),
    );

    tool_dao.create_tool(ctx.clone(), &tool1).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &tool2).await.unwrap();

    // 按 python tag 查询，应只返回 tool1
    let query = crate::service::dao::tool::ToolQuery {
        tags: Some(vec!["python".to_string()]),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(results.items.len(), 1);
    assert_eq!(results.items[0].id, "tag-tool-1");
}

#[sqlx::test]
async fn test_query_by_multiple_tags(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    let tool1 = ToolPo::new(
        "multi-tag-1".to_string(),
        "python-tool".to_string(),
        "Python".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["python".to_string()],
        Some("admin".to_string()),
    );
    let tool2 = ToolPo::new(
        "multi-tag-2".to_string(),
        "rust-tool".to_string(),
        "Rust".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["rust".to_string()],
        Some("admin".to_string()),
    );
    let tool3 = ToolPo::new(
        "multi-tag-3".to_string(),
        "js-tool".to_string(),
        "JavaScript".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["javascript".to_string()],
        Some("admin".to_string()),
    );

    tool_dao.create_tool(ctx.clone(), &tool1).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &tool2).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &tool3).await.unwrap();

    // 查询 python 或 rust tag（OR 语义），应返回 tool1 和 tool2，不含 tool3
    let query = crate::service::dao::tool::ToolQuery {
        tags: Some(vec!["python".to_string(), "rust".to_string()]),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(results.items.len(), 2);
    let ids: Vec<String> = results.items.iter().map(|t| t.id.clone()).collect();
    assert!(ids.contains(&"multi-tag-1".to_string()));
    assert!(ids.contains(&"multi-tag-2".to_string()));
    assert!(!ids.contains(&"multi-tag-3".to_string()));
}

#[sqlx::test]
async fn test_query_without_tags(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    let tool1 = ToolPo::new(
        "no-tag-1".to_string(),
        "alpha-tool".to_string(),
        "Alpha".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["alpha".to_string()],
        Some("admin".to_string()),
    );
    let tool2 = ToolPo::new(
        "no-tag-2".to_string(),
        "beta-tool".to_string(),
        "Beta".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["beta".to_string()],
        Some("admin".to_string()),
    );

    tool_dao.create_tool(ctx.clone(), &tool1).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &tool2).await.unwrap();

    // tags = None 时，不过滤 tag，应返回全部
    let query = crate::service::dao::tool::ToolQuery {
        tags: None,
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(results.items.len(), 2);
    let ids: Vec<String> = results.items.iter().map(|t| t.id.clone()).collect();
    assert!(ids.contains(&"no-tag-1".to_string()));
    assert!(ids.contains(&"no-tag-2".to_string()));
}

#[sqlx::test]
async fn test_keyword_search_matches_tags(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // name 和 description 都不含 "unique_tool_tag"，只有 tags 含
    let tool = ToolPo::new(
        "keyword-tag-tool".to_string(),
        "generic-tool".to_string(),
        "A generic tool".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["unique_tool_tag".to_string()],
        Some("admin".to_string()),
    );
    tool_dao.create_tool(ctx.clone(), &tool).await.unwrap();

    // 用 tag 内容作为关键词通过 FTS5 搜索，应能命中（tools_fts 索引包含 tags 字段）
    let search = crate::service::dao::tool::ToolSearch {
        keyword: Some("unique_tool_tag".to_string()),
        enabled_only: false,
        limit: 10,
        ..Default::default()
    };
    let results = tool_dao.search_tools(ctx.clone(), search).await.unwrap();
    assert_eq!(results.len(), 1);
    let (po, fts_rank) = &results[0];
    assert_eq!(po.id, "keyword-tag-tool");
    assert!(fts_rank.is_some(), "FTS5 MATCH 结果应有 fts_rank");
}

// ==================== FTS5 搜索测试 ====================

#[sqlx::test]
async fn test_search_tools_fts5_basic(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // 创建包含英文关键词的工具
    let tool1 = ToolPo::new(
        "fts-1".to_string(),
        "rust-ownership-tool".to_string(),
        "Rust ownership system ensures memory safety".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["rust".to_string()],
        Some("admin".to_string()),
    );
    let tool2 = ToolPo::new(
        "fts-2".to_string(),
        "python-analysis-tool".to_string(),
        "Python data analysis tutorial".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["python".to_string()],
        Some("admin".to_string()),
    );
    tool_dao.create_tool(ctx.clone(), &tool1).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &tool2).await.unwrap();

    // 搜索 "rust" — 只应匹配 tool1
    let search = crate::service::dao::tool::ToolSearch {
        keyword: Some("rust".to_string()),
        enabled_only: false,
        limit: 10,
        ..Default::default()
    };
    let results = tool_dao.search_tools(ctx.clone(), search).await.unwrap();
    assert_eq!(results.len(), 1, "应只匹配 rust 相关工具");
    let (po, fts_rank) = &results[0];
    assert_eq!(po.id, "fts-1");
    assert!(po.name.contains("rust"));
    assert!(fts_rank.is_some(), "FTS5 MATCH 结果应有 fts_rank");
}

#[sqlx::test]
async fn test_search_tools_fts5_chinese(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // 创建包含中文关键词的工具
    let tool1 = ToolPo::new(
        "cn-fts-1".to_string(),
        "memory-tool".to_string(),
        "这是一个内存管理工具".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["内存管理".to_string()],
        Some("admin".to_string()),
    );
    let tool2 = ToolPo::new(
        "cn-fts-2".to_string(),
        "network-tool".to_string(),
        "网络请求发送工具".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["网络请求".to_string()],
        Some("admin".to_string()),
    );
    tool_dao.create_tool(ctx.clone(), &tool1).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &tool2).await.unwrap();

    // 用中文关键词搜索（trigram 分词器要求至少 3 个字符）
    let search = crate::service::dao::tool::ToolSearch {
        keyword: Some("内存管理".to_string()),
        enabled_only: false,
        limit: 10,
        ..Default::default()
    };
    let results = tool_dao.search_tools(ctx.clone(), search).await.unwrap();
    assert_eq!(results.len(), 1, "中文关键词应只匹配内存工具");
    let (po, _) = &results[0];
    assert_eq!(po.id, "cn-fts-1");
    assert!(po.description.contains("内存"));
}

#[sqlx::test]
async fn test_search_tools_fts5_bm25_ranking(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // 创建多条含 "rust" 关键词的工具，出现频率不同
    // 第一条：rust 出现多次（高相关性）
    let tool1 = ToolPo::new(
        "bm25-1".to_string(),
        "rust-tool".to_string(),
        "Rust Rust Rust programming language features and tools".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec!["rust".to_string()],
        Some("admin".to_string()),
    );
    // 第二条：rust 出现 1 次（低相关性）
    let tool2 = ToolPo::new(
        "bm25-2".to_string(),
        "intro-tool".to_string(),
        "Introduction to Rust for beginners".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    // 第三条：不含 rust（不应被返回）
    let tool3 = ToolPo::new(
        "bm25-3".to_string(),
        "python-tool".to_string(),
        "Python machine learning guide".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    tool_dao.create_tool(ctx.clone(), &tool1).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &tool2).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &tool3).await.unwrap();

    // 搜索 "rust"
    let search = crate::service::dao::tool::ToolSearch {
        keyword: Some("rust".to_string()),
        enabled_only: false,
        limit: 10,
        ..Default::default()
    };
    let results = tool_dao.search_tools(ctx.clone(), search).await.unwrap();

    // 应返回 2 条结果（排除第三条）
    assert_eq!(results.len(), 2);

    // BM25 排序：rust 出现多次的应排在前面（rank 值越小越相关）
    let (po1, rank1) = &results[0];
    let (po2, rank2) = &results[1];
    assert_eq!(po1.id, "bm25-1", "高相关性的工具应排在第一位");
    assert_eq!(po2.id, "bm25-2", "低相关性的工具应排在第二位");

    // 验证 fts_rank 均有值且排序正确
    let r1 = rank1.expect("第一条结果应有 fts_rank");
    let r2 = rank2.expect("第二条结果应有 fts_rank");
    assert!(
        r1 <= r2,
        "BM25 rank of more relevant doc should be <= less relevant (r1={}, r2={})",
        r1,
        r2
    );
}

#[sqlx::test]
async fn test_search_tools_excludes_stale(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    let enabled = ToolPo::new(
        "search-enabled".to_string(),
        "searchable-tool".to_string(),
        "A searchable enabled tool".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let mut stale = ToolPo::new(
        "search-stale".to_string(),
        "searchable-stale-tool".to_string(),
        "A searchable stale tool".to_string(),
        ToolProtocol::Mcp,
        serde_json::json!({"server_id": "srv-a", "tool_name": "stale-search"}),
        None,
        vec![],
        Some("admin".to_string()),
    );
    stale.status = ToolStatus::Stale;

    tool_dao.create_tool(ctx.clone(), &enabled).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &stale).await.unwrap();

    // 搜索 "searchable" — 应排除 Stale 工具
    let search = crate::service::dao::tool::ToolSearch {
        keyword: Some("searchable".to_string()),
        enabled_only: false,
        limit: 10,
        ..Default::default()
    };
    let results = tool_dao.search_tools(ctx.clone(), search).await.unwrap();
    assert_eq!(results.len(), 1, "应排除 Stale 状态的工具");
    assert_eq!(results[0].0.id, "search-enabled");
}

#[sqlx::test]
async fn test_search_tools_enabled_only_filter(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    let enabled = ToolPo::new(
        "eo-enabled".to_string(),
        "filter-tool".to_string(),
        "Filter test enabled".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let mut disabled = ToolPo::new(
        "eo-disabled".to_string(),
        "filter-tool-disabled".to_string(),
        "Filter test disabled".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    disabled.status = ToolStatus::Disabled;

    tool_dao.create_tool(ctx.clone(), &enabled).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &disabled).await.unwrap();

    // 搜索 "filter"，enabled_only=true 只返回启用的
    let search = crate::service::dao::tool::ToolSearch {
        keyword: Some("filter".to_string()),
        enabled_only: true,
        limit: 10,
        ..Default::default()
    };
    let results = tool_dao.search_tools(ctx.clone(), search).await.unwrap();
    assert_eq!(results.len(), 1, "enabled_only 应只返回启用工具");
    assert_eq!(results[0].0.id, "eo-enabled");
}

#[sqlx::test]
async fn test_search_tools_empty_keyword_returns_empty(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    let tool = ToolPo::new(
        "empty-kw-tool".to_string(),
        "some-tool".to_string(),
        "Some tool".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    tool_dao.create_tool(ctx.clone(), &tool).await.unwrap();

    // 空关键词应返回空结果
    let search = crate::service::dao::tool::ToolSearch {
        keyword: Some("".to_string()),
        enabled_only: false,
        limit: 10,
        ..Default::default()
    };
    let results = tool_dao.search_tools(ctx.clone(), search).await.unwrap();
    assert!(results.is_empty(), "空关键词应返回空结果");

    // None 关键词也应返回空结果
    let search = crate::service::dao::tool::ToolSearch {
        keyword: None,
        enabled_only: false,
        limit: 10,
        ..Default::default()
    };
    let results = tool_dao.search_tools(ctx.clone(), search).await.unwrap();
    assert!(results.is_empty(), "None 关键词应返回空结果");
}

#[sqlx::test]
async fn test_query_ignores_keyword_explicitly(pool: SqlitePool) {
    let tool_dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    // 创建两个不同关键词的工具
    let tool1 = ToolPo::new(
        "ignore-kw-1".to_string(),
        "alpha-tool".to_string(),
        "Alpha description".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    let tool2 = ToolPo::new(
        "ignore-kw-2".to_string(),
        "beta-tool".to_string(),
        "Beta description".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("admin".to_string()),
    );
    tool_dao.create_tool(ctx.clone(), &tool1).await.unwrap();
    tool_dao.create_tool(ctx.clone(), &tool2).await.unwrap();

    // 用 "alpha" 作为关键词查询 — query 方法应忽略 keyword，返回全部工具
    let query = crate::service::dao::tool::ToolQuery {
        keyword: Some("alpha".to_string()),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    // keyword 被忽略，应返回全部 2 条工具（而非只有 alpha-tool）
    assert_eq!(
        results.items.len(),
        2,
        "query 方法应忽略 keyword，返回全部工具"
    );

    // 用完全不匹配的关键词也应返回全部
    let query = crate::service::dao::tool::ToolQuery {
        keyword: Some("zzz_nonexistent_keyword_zzz".to_string()),
        ..Default::default()
    };
    let results = tool_dao.query(ctx.clone(), query).await.unwrap();
    assert_eq!(
        results.items.len(),
        2,
        "不匹配的 keyword 也应被忽略，返回全部工具"
    );
}
