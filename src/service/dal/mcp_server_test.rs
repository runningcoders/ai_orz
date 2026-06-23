//! MCP Server DAL contract tests.
//!
//! Batch 1 verifies that upper layers use the `McpServer` business entity while
//! DAO remains a pure `McpServerPo` persistence boundary.

use crate::error::{AppError, Result};
use crate::models::mcp_server::{McpServer, McpServerConfig, McpServerStatus, McpTransport};
use crate::pkg::RequestContext;
use crate::service::dal::mcp_server::{self, McpServerDal};
use crate::service::dao::tool_call::McpToolCallDao;
use crate::service::dao::{mcp_server as mcp_server_dao, tool_call};
use sqlx::SqlitePool;
use std::sync::Arc;

fn stdio_server(name: &str, creator: &str) -> McpServer {
    McpServer::new(
        "".to_string(),
        name.to_string(),
        McpTransport::Stdio,
        McpServerConfig {
            command: Some("npx".to_string()),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-memory".to_string(),
            ],
            ..McpServerConfig::default_stdio()
        },
        Some(creator.to_string()),
    )
}

fn stdio_server_with_id(id: &str, name: &str, creator: &str) -> McpServer {
    let mut server = stdio_server(name, creator);
    server.po.id = id.to_string();
    server
}

fn stdio_server_without_command(id: &str, name: &str, creator: &str) -> McpServer {
    McpServer::new(
        id.to_string(),
        name.to_string(),
        McpTransport::Stdio,
        McpServerConfig {
            command: Some("   ".to_string()),
            ..McpServerConfig::default_stdio()
        },
        Some(creator.to_string()),
    )
}

fn streamable_http_server(id: &str, name: &str, creator: &str) -> McpServer {
    McpServer::new(
        id.to_string(),
        name.to_string(),
        McpTransport::StreamableHttp,
        McpServerConfig {
            url: Some("https://example.com/mcp".to_string()),
            ..McpServerConfig::default_streamable_http()
        },
        Some(creator.to_string()),
    )
}

fn init_test_env(
    pool: SqlitePool,
) -> (
    Arc<dyn McpServerDal + Send + Sync>,
    Arc<dyn McpToolCallDao + Send + Sync>,
    RequestContext,
) {
    let base_tool_call_dao = tool_call::new();
    let mcp_tool_call_dao = tool_call::new_mcp_tool_call_dao(base_tool_call_dao);
    let dal = mcp_server::new(
        mcp_server_dao::new_mcp_server_dao(),
        mcp_tool_call_dao.clone(),
    );
    let ctx = RequestContext::new_simple("test-user", pool);
    (dal, mcp_tool_call_dao, ctx)
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_server_dal_create_persists_stdio_server_and_returns_business_entity(
    pool: SqlitePool,
) -> Result<()> {
    let (dal, _mcp_tool_call_dao, ctx) = init_test_env(pool);
    let server = stdio_server("memory", "test-user");
    let server_id = server.po.id.clone();

    dal.create(ctx.clone(), &server).await?;

    let persisted = dal
        .find_by_id(ctx.clone(), &server_id)
        .await?
        .expect("created MCP server should be returned as business entity");

    assert_eq!(persisted.po.id, server_id);
    assert_eq!(persisted.po.name, "memory");
    assert_eq!(persisted.po.transport, McpTransport::Stdio);
    assert_eq!(persisted.po.config().command, Some("npx".to_string()));
    assert_eq!(persisted.po.created_by, Some("test-user".to_string()));
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_server_dal_derive_audit_fields_from_request_context(pool: SqlitePool) -> Result<()> {
    let (dal, _mcp_tool_call_dao, ctx) = init_test_env(pool);
    let mut server = stdio_server("audit", "spoofed-user");
    let server_id = server.po.id.clone();

    dal.create(ctx.clone(), &server).await?;
    let persisted = dal
        .find_by_id(ctx.clone(), &server_id)
        .await?
        .expect("created server should exist");
    assert_eq!(persisted.po.created_by, Some("test-user".to_string()));
    assert_eq!(persisted.po.updated_by, Some("test-user".to_string()));

    server.po.name = "audit-updated".to_string();
    server.po.updated_by = Some("spoofed-updater".to_string());
    dal.update(ctx.clone(), &server).await?;

    let updated = dal
        .find_by_id(ctx.clone(), &server_id)
        .await?
        .expect("updated server should exist");
    assert_eq!(updated.po.updated_by, Some("test-user".to_string()));
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_server_dal_create_rejects_stdio_without_command_and_does_not_persist(
    pool: SqlitePool,
) -> Result<()> {
    let (dal, _mcp_tool_call_dao, ctx) = init_test_env(pool);
    let server = stdio_server_without_command("invalid-stdio-server", "invalid", "test-user");

    let result = dal.create(ctx.clone(), &server).await;

    assert!(matches!(result, Err(AppError::BadRequest(_))));
    assert!(
        dal.find_by_id(ctx.clone(), "invalid-stdio-server")
            .await?
            .is_none()
    );
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_server_dal_create_rejects_streamable_http_until_security_policy_exists(
    pool: SqlitePool,
) -> Result<()> {
    let (dal, _mcp_tool_call_dao, ctx) = init_test_env(pool);
    let server = streamable_http_server("pending-http-server", "http", "test-user");

    let result = dal.create(ctx.clone(), &server).await;

    assert!(matches!(result, Err(AppError::BadRequest(_))));
    assert!(
        dal.find_by_id(ctx.clone(), "pending-http-server")
            .await?
            .is_none()
    );
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_server_dal_update_status_and_delete_invalidate_mcp_runtime_cache(
    pool: SqlitePool,
) -> Result<()> {
    let (dal, mcp_tool_call_dao, ctx) = init_test_env(pool);
    let mut server = stdio_server_with_id("server-to-invalidate", "memory", "test-user");

    dal.create(ctx.clone(), &server).await?;
    assert!(!mcp_tool_call_dao.is_mcp_server_invalidated("server-to-invalidate"));

    server.po.name = "memory-updated".to_string();
    dal.update(ctx.clone(), &server).await?;
    assert!(mcp_tool_call_dao.is_mcp_server_invalidated("server-to-invalidate"));

    let another_server = stdio_server_with_id("server-status-delete", "memory-2", "test-user");
    dal.create(ctx.clone(), &another_server).await?;
    assert!(!mcp_tool_call_dao.is_mcp_server_invalidated("server-status-delete"));

    dal.set_status(
        ctx.clone(),
        "server-status-delete",
        McpServerStatus::Disabled,
    )
    .await?;
    assert!(mcp_tool_call_dao.is_mcp_server_invalidated("server-status-delete"));

    let deleted_server = stdio_server_with_id("server-delete", "memory-3", "test-user");
    dal.create(ctx.clone(), &deleted_server).await?;
    assert!(!mcp_tool_call_dao.is_mcp_server_invalidated("server-delete"));

    dal.delete(ctx.clone(), "server-delete").await?;
    assert!(mcp_tool_call_dao.is_mcp_server_invalidated("server-delete"));
    Ok(())
}
