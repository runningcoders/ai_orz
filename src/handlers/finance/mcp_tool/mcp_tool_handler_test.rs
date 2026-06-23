use common::api::{ListMcpToolsByServerRequest, SyncMcpToolsRequest};
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use serde_json::json;
use sqlx::SqlitePool;

use crate::error::Result;
use crate::models::mcp_server::{McpServer, McpServerConfig, McpTransport};
use crate::models::tool::ToolPo;
use crate::pkg::RequestContext;
use crate::service::dao::tool;
use crate::service::domain::finance::domain;

use super::list_mcp_tools_by_server::list_mcp_tools_by_server;
use super::sync_mcp_tools::sync_mcp_tools;

fn init_test_singletons() {
    let _ = crate::config::init();
    crate::service::dao::init_all();
    crate::service::dal::init_all();
    crate::service::domain::init_all();
}

fn stdio_server(id: &str) -> McpServer {
    McpServer::new(
        id.to_string(),
        "echo".to_string(),
        McpTransport::Stdio,
        McpServerConfig {
            command: Some("python3".to_string()),
            args: vec!["placeholder-server.py".to_string()],
            ..McpServerConfig::default_stdio()
        },
        Some("test-user".to_string()),
    )
}

fn mcp_tool_po(id: &str, server_id: &str, tool_name: &str) -> ToolPo {
    let mut po = ToolPo::new(
        id.to_string(),
        id.to_string(),
        format!("MCP tool {tool_name}"),
        ToolProtocol::Mcp,
        json!({
            "server_id": server_id,
            "tool_name": tool_name,
        }),
        Some(json!({"type": "object", "properties": {}})),
        vec![
            "mcp".to_string(),
            server_id.to_string(),
            tool_name.to_string(),
        ],
        Some("test-user".to_string()),
    );
    po.control_mode = ControlMode::Manual;
    po
}

fn write_echo_mcp_server_script() -> tempfile::NamedTempFile {
    let script = r#"
import json
import sys

for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    request_id = message.get("id")
    if method == "initialize":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {
                "protocolVersion": "2025-11-25",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "echo-test-server", "version": "1.0.0"},
            },
        }), flush=True)
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {
                "tools": [{
                    "name": "echo",
                    "description": "Echo input text",
                    "inputSchema": {
                        "type": "object",
                        "properties": {"text": {"type": "string"}},
                        "required": ["text"],
                    },
                }]
            },
        }), flush=True)
"#;

    let mut file = tempfile::NamedTempFile::new().expect("temp MCP script should be created");
    std::io::Write::write_all(&mut file, script.as_bytes())
        .expect("temp MCP script should be written");
    file
}

#[sqlx::test(migrations = "./migrations")]
async fn list_mcp_tools_by_server_returns_only_bound_mcp_tools_with_total(
    pool: SqlitePool,
) -> Result<()> {
    init_test_singletons();
    let ctx = RequestContext::new_simple("test-user", pool);
    domain()
        .mcp_server_manage()
        .create_mcp_server(ctx.clone(), &stdio_server("server-a"))
        .await?;

    let mut disabled = mcp_tool_po("mcp.server-a.disabled", "server-a", "disabled");
    disabled.status = ToolStatus::Disabled;
    let records = vec![
        mcp_tool_po("mcp.server-a.echo", "server-a", "echo"),
        mcp_tool_po("mcp.server-a.read", "server-a", "read"),
        disabled,
        mcp_tool_po("mcp.server-b.echo", "server-b", "echo"),
        ToolPo::new_builtin(
            "builtin-test".to_string(),
            "builtin_test".to_string(),
            "Not an MCP tool".to_string(),
        ),
    ];
    for record in records {
        tool::new_tool_dao()
            .create_tool(ctx.clone(), &record)
            .await?;
    }

    let response = list_mcp_tools_by_server(
        ctx,
        ListMcpToolsByServerRequest {
            server_id: "server-a".to_string(),
            status: Some(ToolStatus::Enabled),
            pagination: common::api::PaginationParams {
                limit: None,
                offset: Some(1),
            },
            ..Default::default()
        },
    )
    .await?;

    assert_eq!(response.total, 2);
    assert_eq!(response.tools.len(), 1);
    assert_eq!(response.tools[0].protocol, ToolProtocol::Mcp);
    assert!(response.tools[0].id.starts_with("mcp.server-a."));
    assert!(response.tools[0].has_config);
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn sync_mcp_tools_handler_syncs_remote_tools(pool: SqlitePool) -> Result<()> {
    init_test_singletons();
    let ctx = RequestContext::new_simple("test-user", pool);
    let script = write_echo_mcp_server_script();
    let server = McpServer::new(
        "echo-server".to_string(),
        "echo".to_string(),
        McpTransport::Stdio,
        McpServerConfig {
            command: Some("python3".to_string()),
            args: vec![script.path().to_string_lossy().to_string()],
            ..McpServerConfig::default_stdio()
        },
        Some("test-user".to_string()),
    );
    domain()
        .mcp_server_manage()
        .create_mcp_server(ctx.clone(), &server)
        .await?;

    let response = sync_mcp_tools(
        ctx.clone(),
        SyncMcpToolsRequest {
            server_id: "echo-server".to_string(),
        },
    )
    .await?;

    assert_eq!(response.synced, 1);
    let persisted = tool::new_tool_dao()
        .get_by_id(ctx, "mcp.echo-server.echo".to_string())
        .await?
        .expect("synced MCP tool should be persisted");
    assert_eq!(persisted.protocol, ToolProtocol::Mcp);
    assert_eq!(persisted.control_mode, ControlMode::Manual);
    assert_eq!(persisted.config["server_id"], "echo-server");
    assert_eq!(persisted.config["tool_name"], "echo");
    Ok(())
}
