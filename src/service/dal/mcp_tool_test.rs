//! MCP Tool DAL skeleton contract tests.
//!
//! Stage 3 verifies MCP-specific DAL orchestration: read ToolPo, read
//! McpServerPo, then ask McpToolCallDao for an executable MCP CoreTool.

use crate::error::Result;
use crate::models::mcp_server::{McpServerConfig, McpServerPo, McpTransport};
use crate::models::tool::ToolPo;
use crate::pkg::RequestContext;
use crate::service::dal::mcp_tool::{self, McpToolDal};
use crate::service::dao::{mcp_server, tool, tool_call};
use common::enums::ToolProtocol;
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;

fn mcp_tool_po(server_id: &str, tool_name: &str) -> ToolPo {
    ToolPo::new(
        "mcp-read-file".to_string(),
        "mcp_read_file".to_string(),
        "Read a file through an MCP server".to_string(),
        ToolProtocol::Mcp,
        json!({
            "server_id": server_id,
            "tool_name": tool_name,
        }),
        Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        })),
        vec!["mcp".to_string()],
        Some("test-user".to_string()),
    )
}

fn mcp_server(id: &str) -> McpServerPo {
    McpServerPo::new(
        id.to_string(),
        "filesystem".to_string(),
        McpTransport::Stdio,
        McpServerConfig {
            command: Some("npx".to_string()),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
            ],
            ..McpServerConfig::default_stdio()
        },
        Some("test-user".to_string()),
    )
}

fn init_test_env(pool: SqlitePool) -> (Arc<dyn McpToolDal + Send + Sync>, RequestContext) {
    let base_tool_call_dao = tool_call::new();
    let mcp_tool_call_dao = tool_call::new_mcp_tool_call_dao(base_tool_call_dao);
    let dal = mcp_tool::new(
        tool::new_tool_dao(),
        mcp_server::new_mcp_server_dao(),
        mcp_tool_call_dao,
    );
    let ctx = RequestContext::new_simple("test-user", pool);
    (dal, ctx)
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_get_by_id_assembles_tool_with_server_config(pool: SqlitePool) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let server = mcp_server("filesystem-server");
    let po = mcp_tool_po(&server.id, "read_file");

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;
    tool::new_tool_dao().create_tool(ctx.clone(), &po).await?;

    let tool = dal
        .get_by_id(ctx.clone(), po.id.clone())
        .await?
        .expect("MCP tool should be assembled when server config exists");

    assert_eq!(tool.po.id, po.id);
    assert_eq!(tool.po.protocol, ToolProtocol::Mcp);
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_rejects_non_mcp_tool(pool: SqlitePool) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let po = ToolPo::new_builtin(
        "builtin-test".to_string(),
        "builtin_test".to_string(),
        "Not an MCP tool".to_string(),
    );
    tool::new_tool_dao().create_tool(ctx.clone(), &po).await?;

    let err = dal
        .get_by_id(ctx.clone(), po.id.clone())
        .await
        .expect_err("McpToolDal should reject non-MCP ToolPo records");

    assert!(err.to_string().contains("not an MCP tool"));
    Ok(())
}
