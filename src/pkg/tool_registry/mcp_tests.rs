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

fn write_failing_mcp_server_script(method_to_fail: &str) -> tempfile::NamedTempFile {
    let script = format!(
        r#"
import json
import sys

METHOD_TO_FAIL = {method_to_fail:?}
SENSITIVE_ERROR = "lower layer failed for /opt/private/mcp-server with env API_TOKEN=placeholder-value url=https://example.invalid/mcp?credential=placeholder-value"

for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    request_id = message.get("id")
    if method == "initialize":
        response = {{
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {{
                "protocolVersion": "2025-11-25",
                "capabilities": {{"tools": {{}}}},
                "serverInfo": {{"name": "failing-test-server", "version": "1.0.0"}},
            }},
        }}
        print(json.dumps(response), flush=True)
    elif method == "notifications/initialized":
        continue
    elif method == METHOD_TO_FAIL:
        response = {{
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {{"code": -32000, "message": SENSITIVE_ERROR}},
        }}
        print(json.dumps(response), flush=True)
    elif method == "tools/list":
        response = {{
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {{"tools": [{{"name": "echo", "description": "Echo input text", "inputSchema": {{"type": "object"}}}}]}},
        }}
        print(json.dumps(response), flush=True)
    elif method == "tools/call":
        response = {{
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {{"content": [], "structuredContent": {{}}, "isError": False}},
        }}
        print(json.dumps(response), flush=True)
"#
    );

    let mut file = tempfile::NamedTempFile::new().expect("temp MCP script should be created");
    std::io::Write::write_all(&mut file, script.as_bytes())
        .expect("temp MCP script should be written");
    file
}

fn assert_mcp_runtime_error_is_redacted(message: &str) {
    assert!(!message.contains("/opt/private/mcp-server"));
    assert!(!message.contains("API_TOKEN"));
    assert!(!message.contains("placeholder-value"));
    assert!(!message.contains("credential"));
    assert!(!message.contains("example.invalid"));
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

#[test]
fn mcp_stdio_session_close_failure_message_redacts_lower_layer_details() {
    let message = super::mcp_stdio_session_close_failed_message("private-server", "tool call echo");

    assert!(message.contains("MCP stdio session close failed"));
    assert!(message.contains("private-server"));
    assert!(message.contains("tool call echo"));
    assert!(!message.contains("/opt/private/mcp-server"));
    assert!(!message.contains("API_TOKEN"));
    assert!(!message.contains("placeholder-value"));
    assert!(!message.contains("credential"));
}

#[tokio::test]
async fn mcp_client_runtime_rejects_non_object_args_without_leaking_args() {
    let runtime = McpClientRuntime::default();
    let server = mcp_server_with_command(
        "echo-server",
        "mcp-command-with-credential-placeholder-value".to_string(),
        Vec::new(),
    );

    let error = runtime
        .call_tool(
            &server,
            "echo",
            json!("/opt/private/mcp-server?credential=placeholder-value"),
        )
        .await
        .expect_err("non-object MCP args should be rejected before spawning stdio process");

    let message = error.to_string();
    assert!(message.contains("MCP tool arguments must be a JSON object"));
    assert_mcp_runtime_error_is_redacted(&message);
}

#[tokio::test]
async fn mcp_client_runtime_redacts_stdio_command_resolution_errors() {
    let runtime = McpClientRuntime::default();
    let server = mcp_server_with_command(
        "echo-server",
        "mcp-command-with-credential-placeholder-value".to_string(),
        Vec::new(),
    );

    let error = runtime
        .list_tools(&server)
        .await
        .expect_err("missing MCP stdio command should fail safely");

    let message = error.to_string();
    assert!(message.contains("MCP stdio command was not found in PATH"));
    assert_mcp_runtime_error_is_redacted(&message);
}

#[tokio::test]
async fn mcp_client_runtime_redacts_stdio_tools_list_lower_layer_errors() {
    let script = write_failing_mcp_server_script("tools/list");
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );
    let runtime = McpClientRuntime::default();

    let error = runtime
        .list_tools(&server)
        .await
        .expect_err("MCP tools/list lower-layer error should fail safely");

    let message = error.to_string();
    assert!(message.contains("MCP tools/list on server echo-server failed"));
    assert_mcp_runtime_error_is_redacted(&message);
}

#[tokio::test]
async fn mcp_client_runtime_redacts_stdio_tool_call_lower_layer_errors() {
    let script = write_failing_mcp_server_script("tools/call");
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );
    let runtime = McpClientRuntime::default();

    let error = runtime
        .call_tool(&server, "echo", json!({ "text": "hello" }))
        .await
        .expect_err("MCP tools/call lower-layer error should fail safely");

    let message = error.to_string();
    assert!(message.contains("MCP tool echo on server echo-server call failed"));
    assert_mcp_runtime_error_is_redacted(&message);
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

#[tokio::test]
async fn mcp_client_runtime_consumes_invalidation_on_next_stdio_call() {
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );
    let runtime = McpClientRuntime::default();

    runtime.invalidate_server(&server.id);
    assert!(runtime.is_invalidated(&server.id));

    let result = runtime
        .call_tool(&server, "echo", json!({ "text": "after invalidate" }))
        .await
        .expect("next MCP stdio call after invalidation should reconnect and execute");

    assert_eq!(result["structuredContent"]["echo"], "after invalidate");
    assert_eq!(result["isError"], false);
    assert!(
        !runtime.is_invalidated(&server.id),
        "successful next call should consume the invalidation marker"
    );
}

#[tokio::test]
async fn mcp_client_runtime_allows_concurrent_stdio_calls_to_same_server() {
    let script = write_echo_mcp_server_script();
    let server = Arc::new(mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    ));
    let runtime = Arc::new(McpClientRuntime::default());

    let first_runtime = runtime.clone();
    let first_server = server.clone();
    let second_runtime = runtime.clone();
    let second_server = server.clone();

    let (first, second) = tokio::join!(
        async move {
            first_runtime
                .call_tool(&first_server, "echo", json!({ "text": "first" }))
                .await
        },
        async move {
            second_runtime
                .call_tool(&second_server, "echo", json!({ "text": "second" }))
                .await
        }
    );

    let first = first.expect("first concurrent MCP stdio call should succeed");
    let second = second.expect("second concurrent MCP stdio call should succeed");

    assert_eq!(first["structuredContent"]["echo"], "first");
    assert_eq!(second["structuredContent"]["echo"], "second");
}
