//! McpServer DAO SQLite 单元测试
//!
//! Stage 1 只验证 MCP Server 配置持久化，不启动 MCP client/session。

use crate::error::Result;
use crate::models::mcp_server::{McpServerConfig, McpServerPo, McpServerStatus, McpTransport};
use crate::pkg::RequestContext;
use crate::service::dao::mcp_server::{self, McpServerDao, McpServerQuery};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::sync::Arc;

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
    assert_eq!(default_query.len(), 1);
    assert_eq!(default_query[0].id, http_server.id);

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
    assert!(enabled_stdio.is_empty());

    let all_non_deleted = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                exclude_status: Some(McpServerStatus::Deleted),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(all_non_deleted.len(), 1);

    let deleted_stdio = dao
        .query(
            ctx.clone(),
            McpServerQuery {
                status: Some(McpServerStatus::Deleted),
                offset: Some(0),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(deleted_stdio.len(), 1);
    assert_eq!(deleted_stdio[0].id, stdio_server.id);

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
    assert_eq!(deleted_rows.len(), 1);

    let recreated = create_stdio_server("filesystem-updated", "test-user");
    dao.insert(ctx.clone(), &recreated).await?;
    let active_rows = dao.query(ctx.clone(), McpServerQuery::default()).await?;
    assert_eq!(active_rows.len(), 1);
    assert_eq!(active_rows[0].id, recreated.id);

    Ok(())
}
