//! McpServer DAO SQLite 单元测试
//!
//! Stage 1 只验证 MCP Server 配置持久化，不启动 MCP client/session。

use crate::models::mcp_server::McpServerQuery;
use crate::models::mcp_server::{McpServerConfig, McpServerPo, McpServerStatus, McpTransport};
use crate::pkg::RequestContext;
use crate::service::dao::mcp_server::{self, McpServerDao};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::sync::Arc;
use common::error::Result;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

fn init_test_env(pool: SqlitePool) -> (Arc<dyn McpServerDao + Send + Sync>, RequestContext) {
    let dao = mcp_server::new_mcp_server_dao();
    let ctx = new_ctx("test-user", pool);
    (dao, ctx)
}

fn create_stdio_server(name: &str, creator: &str) -> McpServerPo {
    let config = McpServerConfig {
        command: Some("npx".to_string()),
        args: vec![
            "-y".to_string(),
            "@modelcontextprotocol/server-filesystem".to_string(),
        ],
        env: BTreeMap::from([("MCP_TEST_ENV".to_string(), "test-value".to_string())]),
        timeout_ms: 30_000,
        ..McpServerConfig::default_stdio()
    };

    McpServerPo::new(
        "".to_string(),
        name.to_string(),
        McpTransport::Stdio,
        config,
        Some(creator.to_string()),
    )
}

fn create_http_server(name: &str, creator: &str) -> McpServerPo {
    let config = McpServerConfig {
        url: Some("https://mcp.example.com/mcp".to_string()),
        headers: BTreeMap::from([("X-Test-Header".to_string(), "test-value".to_string())]),
        timeout_ms: 30_000,
        ..McpServerConfig::default_streamable_http()
    };

    McpServerPo::new(
        "".to_string(),
        name.to_string(),
        McpTransport::StreamableHttp,
        config,
        Some(creator.to_string()),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn test_insert_and_find_by_id(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let server = create_stdio_server("filesystem", "test-user");

    dao.insert(ctx.clone(), &server).await?;

    let found = dao.find_by_id(ctx.clone(), &server.id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, server.id);
    assert_eq!(found.name, "filesystem");
    assert_eq!(found.transport, McpTransport::Stdio);
    assert_eq!(found.status, McpServerStatus::Enabled);
    assert_eq!(found.config().command.as_deref(), Some("npx"));
    assert_eq!(
        found.config().env.get("MCP_TEST_ENV"),
        Some(&"test-value".to_string())
    );

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_query_by_transport_and_status(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let stdio_server = create_stdio_server("filesystem", "test-user");
    let mut http_server = create_http_server("remote-search", "test-user");
    http_server.status = McpServerStatus::Disabled;

    dao.insert(ctx.clone(), &stdio_server).await?;
    dao.insert(ctx.clone(), &http_server).await?;
    dao.delete(ctx.clone(), &stdio_server.id).await?;

    let default_query = dao.query(ctx.clone(), McpServerQuery::default()).await?;
    assert_eq!(default_query.items.len(), 1);
    assert_eq!(default_query.items[0].id, http_server.id);
    assert_eq!(default_query.total, 1);

    let enabled_stdio = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                transport: Some(McpTransport::Stdio),
                status: Some(McpServerStatus::Enabled),
                ..Default::default()
            },
        )
        .await?;
    assert!(enabled_stdio.items.is_empty());
    assert_eq!(enabled_stdio.total, 0);

    let all_non_deleted = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                exclude_status: Some(McpServerStatus::Deleted),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(all_non_deleted.items.len(), 1);
    assert_eq!(all_non_deleted.total, 1);

    let deleted_stdio = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                status: Some(McpServerStatus::Deleted),
                pagination: common::api::PaginationParams {
                    offset: Some(0),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(deleted_stdio.items.len(), 1);
    assert_eq!(deleted_stdio.items[0].id, stdio_server.id);
    assert_eq!(deleted_stdio.total, 1);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_query_returns_page_items_and_total_with_unified_pagination(
    pool: SqlitePool,
) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let server_a = create_stdio_server("page-a", "test-user");
    let server_b = create_stdio_server("page-b", "test-user");
    let mut server_c = create_stdio_server("page-c", "test-user");
    server_c.status = McpServerStatus::Disabled;

    dao.insert(ctx.clone(), &server_a).await?;
    dao.insert(ctx.clone(), &server_b).await?;
    dao.insert(ctx.clone(), &server_c).await?;

    let page = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                transport: Some(McpTransport::Stdio),
                pagination: common::api::PaginationParams {
                    limit: Some(1),
                    offset: Some(1),
                },
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total, 3);

    let enabled_total = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                status: Some(McpServerStatus::Enabled),
                pagination: common::api::PaginationParams {
                    limit: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(enabled_total.items.len(), 1);
    assert_eq!(enabled_total.total, 2);

    dao.delete(ctx.clone(), &server_a.id).await?;
    let non_deleted_total = dao.query(ctx.clone(), McpServerQuery::default()).await?;
    assert_eq!(non_deleted_total.items.len(), 2);
    assert_eq!(non_deleted_total.total, 2);

    let offset_only_page = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                pagination: common::api::PaginationParams {
                    offset: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(offset_only_page.items.len(), 1);
    assert_eq!(offset_only_page.total, 2);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_set_status_does_not_restore_soft_deleted_server(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let server = create_stdio_server("filesystem", "test-user");

    dao.insert(ctx.clone(), &server).await?;
    dao.delete(ctx.clone(), &server.id).await?;
    dao.set_status(ctx.clone(), &server.id, McpServerStatus::Enabled)
        .await?;

    let active_rows = dao.query(ctx.clone(), McpServerQuery::default()).await?;
    assert!(active_rows.items.is_empty());

    let deleted_rows = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                id: Some(server.id),
                status: Some(McpServerStatus::Deleted),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(deleted_rows.items.len(), 1);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update_and_soft_delete(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let mut server = create_stdio_server("filesystem", "test-user");
    dao.insert(ctx.clone(), &server).await?;

    server.name = "filesystem-updated".to_string();
    server.status = McpServerStatus::Disabled;
    server.touch(Some("modifier".to_string()));
    dao.update(ctx.clone(), &server).await?;

    let found = dao.find_by_id(ctx.clone(), &server.id).await?.unwrap();
    assert_eq!(found.name, "filesystem-updated");
    assert_eq!(found.status, McpServerStatus::Disabled);
    assert_eq!(found.updated_by.as_deref(), Some("modifier"));

    dao.delete(ctx.clone(), &server.id).await?;
    let deleted = dao.find_by_id(ctx.clone(), &server.id).await?;
    assert!(deleted.is_none());

    let deleted_rows = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                id: Some(server.id.clone()),
                status: Some(McpServerStatus::Deleted),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(deleted_rows.items.len(), 1);
    assert_eq!(deleted_rows.total, 1);

    let recreated = create_stdio_server("filesystem-updated", "test-user");
    dao.insert(ctx.clone(), &recreated).await?;
    let active_rows = dao.query(ctx.clone(), McpServerQuery::default()).await?;
    assert_eq!(active_rows.items.len(), 1);
    assert_eq!(active_rows.items[0].id, recreated.id);
    assert_eq!(active_rows.total, 1);

    Ok(())
}
