//! MCP ToolCall DAO skeleton contract tests.
//!
//! Stage 3 verifies that MCP runtime concerns stay in the MCP-specific
//! ToolCall DAO instead of leaking into the generic ToolCallDao entrypoint.

use crate::models::mcp_server::{McpServerConfig, McpServerPo, McpTransport};
use crate::models::tool::ToolPo;
use crate::service::dao::tool_call::{self, McpToolCallDao, ToolCallDao};
use common::enums::ToolProtocol;
use serde_json::json;

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

#[test]
fn mcp_tool_call_generic_entry_returns_none_without_server_context() {
    let base = tool_call::new();
    let mcp_tool_call_dao = tool_call::new_mcp_tool_call_dao(base);
    let po = mcp_tool_po("filesystem-server", "read_file");

    let assembled = mcp_tool_call_dao
        .assemble_core_tool(&po)
        .expect("generic assemble should not fail");

    assert!(
        assembled.is_none(),
        "generic ToolCallDao entrypoint must not build MCP tools without server/runtime deps"
    );
}

#[test]
fn mcp_tool_call_assembles_mcp_core_tool_with_server_runtime_deps() {
    let base = tool_call::new();
    let mcp_tool_call_dao = tool_call::new_mcp_tool_call_dao(base);
    let po = mcp_tool_po("filesystem-server", "read_file");
    let server = mcp_server("filesystem-server");

    let assembled = mcp_tool_call_dao
        .assemble_mcp_core_tool(&po, &server)
        .expect("mcp-specific assemble should succeed")
        .expect("valid MCP tool/server config should create a CoreTool");

    assert_eq!(assembled.po().id, po.id);
    assert_eq!(assembled.po().protocol, ToolProtocol::Mcp);
}
