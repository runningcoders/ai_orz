use super::{McpClientRuntime, McpToolConfig, McpToolDeps, create_mcp_tool, create_tool};
use crate::models::mcp_server::{McpServerConfig, McpServerPo, McpTransport};
use crate::models::tool::ToolPo;
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::ToolRegistry;
use common::enums::tool::ControlMode;
use common::enums::{ToolProtocol, ToolStatus};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;

fn mcp_tool_po(config: serde_json::Value) -> ToolPo {
    let mut po = ToolPo::new(
        "filesystem_read_file".to_string(),
        "filesystem_read_file".to_string(),
        "Read a file through an MCP filesystem server".to_string(),
        ToolProtocol::Mcp,
        config,
        Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        })),
        vec!["mcp".to_string(), "filesystem".to_string()],
        Some("test".to_string()),
    );
    po.status = ToolStatus::Enabled;
    po
}

#[test]
fn mcp_tool_po_defaults_to_manual_control_mode() {
    let po = ToolPo::new(
        String::new(),
        "default-manual-mcp-tool".to_string(),
        "MCP tools should default to manual control mode".to_string(),
        ToolProtocol::Mcp,
        json!({
            "server_id": "filesystem-server",
            "tool_name": "read_file"
        }),
        None,
        Vec::new(),
        Some("test".to_string()),
    );

    assert_eq!(po.control_mode, ControlMode::Manual);
}

#[test]
fn mcp_tool_config_roundtrips_from_tool_po_config_without_server_credentials() {
    let po = mcp_tool_po(json!({
        "server_id": "filesystem-server",
        "tool_name": "read_file"
    }));

    let config: McpToolConfig = serde_json::from_value(po.config.clone())
        .expect("ToolPo.config should deserialize into McpToolConfig");

    assert_eq!(config.server_id, "filesystem-server");
    assert_eq!(config.tool_name, "read_file");
}

#[test]
fn registry_creates_mcp_core_tool_stub_from_binding_config() {
    let po = mcp_tool_po(json!({
        "server_id": "filesystem-server",
        "tool_name": "read_file"
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Mcp protocol ToolPo should create an executable McpCoreTool stub");

    assert_eq!(tool.po().protocol, ToolProtocol::Mcp);
    assert_eq!(tool.po().control_mode, ControlMode::Manual);
    assert_eq!(tool.po().name, "filesystem_read_file");
}

#[test]
fn registry_rejects_mcp_tool_config_missing_required_binding_fields() {
    let registry = ToolRegistry::default();

    let missing_tool_name = mcp_tool_po(json!({
        "server_id": "filesystem-server"
    }));
    assert!(
        registry.create_tool(missing_tool_name).is_none(),
        "MCP config without tool_name must not create an executable tool"
    );

    let blank_server_id = mcp_tool_po(json!({
        "server_id": " ",
        "tool_name": "read_file"
    }));
    assert!(
        registry.create_tool(blank_server_id).is_none(),
        "MCP config with blank server_id must not create an executable tool"
    );
}

#[test]
fn registry_rejects_mcp_tool_config_that_duplicates_server_credentials() {
    let po = mcp_tool_po(json!({
        "server_id": "filesystem-server",
        "tool_name": "read_file",
        "server_config": {
            "transport": "stdio"
        }
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "MCP ToolPo.config must only bind server_id + tool_name and must not duplicate server credentials"
    );
}

#[test]
fn mcp_factory_rejects_non_mcp_tool_po() {
    let po = ToolPo::new(
        "not_mcp".to_string(),
        "not_mcp".to_string(),
        "Non-MCP tool must not be accepted by the MCP factory".to_string(),
        ToolProtocol::Builtin,
        json!({
            "server_id": "filesystem-server",
            "tool_name": "read_file"
        }),
        None,
        Vec::new(),
        Some("test".to_string()),
    );

    assert!(
        create_tool(po).is_err(),
        "MCP factory should fail closed when called with a non-MCP ToolPo"
    );
}

fn mcp_server_with_command(id: &str, command: String, args: Vec<String>) -> McpServerPo {
    McpServerPo::new(
        id.to_string(),
        "stdio-test-server".to_string(),
        McpTransport::Stdio,
        McpServerConfig {
            command: Some(command),
            args,
            ..McpServerConfig::default_stdio()
        },
        Some("test".to_string()),
    )
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
        response = {
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {
                "protocolVersion": "2025-11-25",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "echo-test-server", "version": "1.0.0"},
            },
        }
        print(json.dumps(response), flush=True)
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        response = {
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {
                "tools": [
                    {
                        "name": "echo",
                        "description": "Echo input text",
                        "inputSchema": {
                            "type": "object",
                            "properties": {"text": {"type": "string"}},
                            "required": ["text"],
                        },
                    }
                ]
            },
        }
        print(json.dumps(response), flush=True)
    elif method == "tools/call":
        args = message.get("params", {}).get("arguments", {})
        text = args.get("text", "")
        response = {
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {
                "content": [{"type": "text", "text": text}],
                "structuredContent": {"echo": text},
                "isError": False,
            },
        }
        print(json.dumps(response), flush=True)
"#;

    let mut file = tempfile::NamedTempFile::new().expect("temp MCP script should be created");
    std::io::Write::write_all(&mut file, script.as_bytes())
        .expect("temp MCP script should be written");
    file
}

#[tokio::test]
async fn mcp_core_tool_calls_stdio_server_through_rmcp_runtime() {
    let script = write_echo_mcp_server_script();
    let po = mcp_tool_po(json!({
        "server_id": "echo-server",
        "tool_name": "echo"
    }));
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );
    let runtime = Arc::new(McpClientRuntime::default());
    let tool = create_mcp_tool(
        po,
        McpToolDeps {
            server,
            client_runtime: runtime,
        },
    )
    .expect("MCP tool with runtime deps should be created");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = RequestContext::new_simple("test-user", pool);

    let result = tool
        .call(ctx, json!({ "text": "hello MCP" }))
        .await
        .expect("MCP stdio tool should execute through rmcp runtime");

    assert_eq!(result["structuredContent"]["echo"], "hello MCP");
    assert_eq!(result["isError"], false);
}

#[tokio::test]
async fn mcp_client_runtime_lists_stdio_server_tools() {
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );
    let runtime = McpClientRuntime::default();

    let tools = runtime
        .list_tools(&server)
        .await
        .expect("MCP stdio runtime should list remote tools");

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    assert_eq!(tools[0].description.as_deref(), Some("Echo input text"));
    assert_eq!(tools[0].input_schema["type"], "object");
    assert_eq!(
        tools[0].input_schema["properties"]["text"]["type"],
        "string"
    );
}
