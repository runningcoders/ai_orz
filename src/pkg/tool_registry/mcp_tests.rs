use super::{
    McpClientRuntime, McpCredentialInjection, McpToolConfig, McpToolDeps, create_mcp_tool,
    create_tool,
};
use crate::models::mcp_server::{McpServerConfig, McpServerPo, McpTransport};
use crate::models::tool::CoreTool;
use crate::models::tool::ToolPo;
use crate::pkg::credential::ResolvedRequirement;
use crate::pkg::tool_registry::ToolRegistry;
use common::enums::tool::ControlMode;
use common::enums::{ToolProtocol, ToolStatus};
use common::models::{CredentialBinding, CredentialKind, CredentialRequirement};
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
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let result = tool
        .call(ctx, json!({ "text": "hello MCP" }))
        .await
        .expect("MCP stdio tool should execute through rmcp runtime");

    assert_eq!(result["structuredContent"]["echo"], "hello MCP");
    assert_eq!(result["isError"], false);
}

#[test]
fn mcp_stdio_session_close_failure_message_redacts_lower_layer_details() {
    let message =
        super::mcp_session_close_failed_message("stdio", "private-server", "tool call echo");

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
            &[],
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
        .list_tools(&server, &[])
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
        .list_tools(&server, &[])
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
        .call_tool(&server, "echo", json!({ "text": "hello" }), &[])
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
        .list_tools(&server, &[])
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
        .call_tool(&server, "echo", json!({ "text": "after invalidate" }), &[])
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
                .call_tool(&first_server, "echo", json!({ "text": "first" }), &[])
                .await
        },
        async move {
            second_runtime
                .call_tool(&second_server, "echo", json!({ "text": "second" }), &[])
                .await
        }
    );

    let first = first.expect("first concurrent MCP stdio call should succeed");
    let second = second.expect("second concurrent MCP stdio call should succeed");

    assert_eq!(first["structuredContent"]["echo"], "first");
    assert_eq!(second["structuredContent"]["echo"], "second");
}

// ==================== 凭据注入生命周期（D22/D23） ====================

/// 回显 MCP_INJECTED_TOKEN 环境变量的 echo 脚本（tools/call 返回 env 值而非入参）
fn write_env_echo_mcp_server_script() -> tempfile::NamedTempFile {
    let script = r#"
import json
import os
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
                "serverInfo": {"name": "env-echo-test-server", "version": "1.0.0"},
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
                        "description": "Echo injected env token",
                        "inputSchema": {"type": "object"},
                    }
                ]
            },
        }
        print(json.dumps(response), flush=True)
    elif method == "tools/call":
        token = os.environ.get("MCP_INJECTED_TOKEN", "<missing>")
        response = {
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {
                "content": [{"type": "text", "text": token}],
                "structuredContent": {"echo": token},
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

fn env_requirement() -> CredentialRequirement {
    CredentialRequirement {
        kind: CredentialKind::GenericToken,
        platform: Some("linear".to_string()),
        field: None,
        enhancer: None,
        binding: CredentialBinding::Env {
            name: "MCP_INJECTED_TOKEN".to_string(),
        },
    }
}

/// 返回 (server, script)：脚本临时文件必须由调用方持有，
/// 否则 NamedTempFile drop 即删除文件，子进程启动失败
fn env_echo_server() -> (McpServerPo, tempfile::NamedTempFile) {
    let script = write_env_echo_mcp_server_script();
    let server = McpServerPo::new(
        "env-echo-server".to_string(),
        "env-echo-test".to_string(),
        McpTransport::Stdio,
        McpServerConfig {
            command: Some("python3".to_string()),
            args: vec![script.path().to_string_lossy().to_string()],
            credential_requirements: vec![env_requirement()],
            ..McpServerConfig::default_stdio()
        },
        Some("test".to_string()),
    );
    (server, script)
}

async fn test_ctx() -> crate::pkg::RequestContext {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

/// 声明来自 server config：credential_requirements() 透传（D17）
#[test]
fn mcp_core_tool_credential_requirements_come_from_server_config() {
    let po = mcp_tool_po(json!({
        "server_id": "env-echo-server",
        "tool_name": "echo"
    }));
    let runtime = Arc::new(McpClientRuntime::default());
    let (server, _script) = env_echo_server();
    let tool = create_mcp_tool(
        po,
        McpToolDeps {
            server,
            client_runtime: runtime,
        },
    )
    .expect("MCP tool should be created");

    let requirements = tool.credential_requirements();
    assert_eq!(requirements.len(), 1);
    assert_eq!(requirements[0].kind, CredentialKind::GenericToken);
    assert_eq!(requirements[0].platform.as_deref(), Some("linear"));
}

/// check 注入 -> 子进程 env -> 工具读到注入值（D22 全链：值来自编排层，不经 DB）
#[tokio::test]
async fn stdio_tool_injects_resolved_env_through_check_lifecycle() {
    let po = mcp_tool_po(json!({
        "server_id": "env-echo-server",
        "tool_name": "echo"
    }));
    let runtime = Arc::new(McpClientRuntime::default());
    let (server, _script) = env_echo_server();
    let mut tool = create_mcp_tool(
        po,
        McpToolDeps {
            server,
            client_runtime: runtime,
        },
    )
    .expect("MCP tool should be created");

    tool.check(&[ResolvedRequirement {
        requirement: env_requirement(),
        value: "secret-token-abc".to_string(),
    }])
    .expect("check should accept Env binding injections");

    let result = tool
        .call(test_ctx().await, json!({ "text": "ignored" }))
        .await
        .expect("MCP stdio tool should execute with injected env");

    assert_eq!(result["structuredContent"]["echo"], "secret-token-abc");
}

/// D23 连接隔离：同 server 两实例注入各自凭据值，互不串扰
///（每调用独立连接 + 实例级注入 = 结构性 (server, 调用者) 隔离）
#[tokio::test]
async fn stdio_tool_injections_are_isolated_per_instance() {
    let po_a = mcp_tool_po(json!({
        "server_id": "env-echo-server",
        "tool_name": "echo"
    }));
    let po_b = mcp_tool_po(json!({
        "server_id": "env-echo-server",
        "tool_name": "echo"
    }));
    let runtime = Arc::new(McpClientRuntime::default());
    let (server, _script) = env_echo_server();
    let mut tool_a = create_mcp_tool(
        po_a,
        McpToolDeps {
            server: server.clone(),
            client_runtime: runtime.clone(),
        },
    )
    .expect("MCP tool A should be created");
    let mut tool_b = create_mcp_tool(
        po_b,
        McpToolDeps {
            server,
            client_runtime: runtime,
        },
    )
    .expect("MCP tool B should be created");

    tool_a
        .check(&[ResolvedRequirement {
            requirement: env_requirement(),
            value: "token-user-a".to_string(),
        }])
        .unwrap();
    tool_b
        .check(&[ResolvedRequirement {
            requirement: env_requirement(),
            value: "token-user-b".to_string(),
        }])
        .unwrap();

    let result_a = tool_a
        .call(test_ctx().await, json!({ "text": "x" }))
        .await
        .expect("tool A should execute");
    let result_b = tool_b
        .call(test_ctx().await, json!({ "text": "x" }))
        .await
        .expect("tool B should execute");

    assert_eq!(result_a["structuredContent"]["echo"], "token-user-a");
    assert_eq!(result_b["structuredContent"]["echo"], "token-user-b");
}

/// check 防御：stdio MCP 收到非 Env binding 的注入值 -> Err（配置期已拦截，此处兜底）
#[tokio::test]
async fn check_rejects_non_env_binding_for_stdio_mcp() {
    let po = mcp_tool_po(json!({
        "server_id": "env-echo-server",
        "tool_name": "echo"
    }));
    let runtime = Arc::new(McpClientRuntime::default());
    let (server, _script) = env_echo_server();
    let mut tool = create_mcp_tool(
        po,
        McpToolDeps {
            server,
            client_runtime: runtime,
        },
    )
    .expect("MCP tool should be created");

    let header_binding = ResolvedRequirement {
        requirement: CredentialRequirement {
            kind: CredentialKind::GenericToken,
            platform: Some("linear".to_string()),
            field: None,
            enhancer: None,
            binding: CredentialBinding::Header {
                name: "Authorization".to_string(),
            },
        },
        value: "Bearer x".to_string(),
    };

    let err = tool
        .check(&[header_binding])
        .expect_err("stdio MCP check must reject non-Env bindings");
    assert!(err.to_string().contains("仅支持 env"));
}

/// 默认 check：未声明凭据需求的工具收到注入值 -> 防御性 Err（编排层错配）
/// 用最小 CoreTool 实现（不覆盖 credential_requirements/check）验证 trait 默认语义
#[tokio::test]
async fn default_check_rejects_unexpected_injections() {
    #[derive(Clone)]
    struct NoCredentialTool {
        po: ToolPo,
    }

    #[async_trait::async_trait]
    impl CoreTool for NoCredentialTool {
        async fn call(
            &self,
            _ctx: crate::pkg::RequestContext,
            _args: serde_json::Value,
        ) -> common::error::Result<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }

        fn po(&self) -> &ToolPo {
            &self.po
        }
    }

    let po = ToolPo::new(
        "builtin_no_creds".to_string(),
        "builtin_no_creds".to_string(),
        "builtin tool without credential requirements".to_string(),
        ToolProtocol::Builtin,
        json!({}),
        None,
        Vec::new(),
        Some("test".to_string()),
    );
    let mut tool = NoCredentialTool { po };

    // 未覆盖 credential_requirements -> 默认空声明
    assert!(tool.credential_requirements().is_empty());
    // 空注入 -> Ok（无凭据工具的正常路径）
    tool.check(&[])
        .expect("empty injection should pass default check");
    // 非空注入 -> 防御性 Err（编排层错配）
    let err = tool
        .check(&[ResolvedRequirement {
            requirement: env_requirement(),
            value: "unexpected".to_string(),
        }])
        .expect_err("default check must reject unexpected injections");
    assert!(err.to_string().contains("未声明凭据需求"));
}

// ==================== 拓展单1：McpHttp scope Header/Query 校验分派 ====================

fn http_header_requirement() -> CredentialRequirement {
    CredentialRequirement {
        kind: CredentialKind::GenericToken,
        platform: Some("linear".to_string()),
        field: None,
        enhancer: None,
        binding: CredentialBinding::Header {
            name: "Authorization".to_string(),
        },
    }
}

fn http_query_requirement() -> CredentialRequirement {
    CredentialRequirement {
        kind: CredentialKind::GenericToken,
        platform: Some("amap".to_string()),
        field: None,
        enhancer: None,
        binding: CredentialBinding::Query {
            name: "key".to_string(),
        },
    }
}

fn streamable_http_server() -> McpServerPo {
    McpServerPo::new(
        "http-echo-server".to_string(),
        "http-echo-test".to_string(),
        McpTransport::StreamableHttp,
        McpServerConfig {
            url: Some("https://mcp.example.invalid/mcp".to_string()),
            ..McpServerConfig::default_streamable_http()
        },
        Some("test".to_string()),
    )
}

/// StreamableHttp MCP：check 接受 Header + Query 双形态（拓展单1；HTTP runtime 消费由拓展单2 落地）
#[test]
fn streamable_http_check_accepts_header_and_query_bindings() {
    let po = mcp_tool_po(json!({
        "server_id": "http-echo-server",
        "tool_name": "echo"
    }));
    let runtime = Arc::new(McpClientRuntime::default());
    let server = streamable_http_server();
    let mut tool = create_mcp_tool(
        po,
        McpToolDeps {
            server,
            client_runtime: runtime,
        },
    )
    .expect("MCP tool should be created");

    tool.check(&[
        ResolvedRequirement {
            requirement: http_header_requirement(),
            value: "Bearer x".to_string(),
        },
        ResolvedRequirement {
            requirement: http_query_requirement(),
            value: "<AMAP_KEY>".to_string(),
        },
    ])
    .expect("StreamableHttp MCP check must accept Header and Query bindings");
}

/// StreamableHttp MCP：check 拒绝 Env 注入（防御兜底）
#[test]
fn streamable_http_check_rejects_env_binding() {
    let po = mcp_tool_po(json!({
        "server_id": "http-echo-server",
        "tool_name": "echo"
    }));
    let runtime = Arc::new(McpClientRuntime::default());
    let server = streamable_http_server();
    let mut tool = create_mcp_tool(
        po,
        McpToolDeps {
            server,
            client_runtime: runtime,
        },
    )
    .expect("MCP tool should be created");

    let err = tool
        .check(&[ResolvedRequirement {
            requirement: env_requirement(),
            value: "x".to_string(),
        }])
        .expect_err("StreamableHttp MCP check must reject Env bindings");
    assert!(err.to_string().contains("不匹配"), "{err}");
}

/// stdio MCP：check 仍拒绝 Query 注入（既有口径回归）
#[test]
fn stdio_check_still_rejects_query_binding() {
    let po = mcp_tool_po(json!({
        "server_id": "env-echo-server",
        "tool_name": "echo"
    }));
    let runtime = Arc::new(McpClientRuntime::default());
    let (server, _script) = env_echo_server();
    let mut tool = create_mcp_tool(
        po,
        McpToolDeps {
            server,
            client_runtime: runtime,
        },
    )
    .expect("MCP tool should be created");

    let err = tool
        .check(&[ResolvedRequirement {
            requirement: http_query_requirement(),
            value: "x".to_string(),
        }])
        .expect_err("stdio MCP check must reject Query bindings");
    assert!(err.to_string().contains("仅支持 env"), "{err}");
}

// ==================== 拓展单2：StreamableHTTP runtime（连接/注入/脱敏/失败路径） ====================

fn streamable_http_server_with_url(url: Option<String>) -> McpServerPo {
    McpServerPo::new(
        "http-echo-server".to_string(),
        "http-echo-test".to_string(),
        McpTransport::StreamableHttp,
        McpServerConfig {
            url,
            ..McpServerConfig::default_streamable_http()
        },
        Some("test".to_string()),
    )
}

fn http_credential_injections() -> Vec<McpCredentialInjection> {
    vec![
        McpCredentialInjection::Header {
            name: "Authorization".to_string(),
            value: "Bearer <AMAP_KEY>".to_string(),
        },
        McpCredentialInjection::Query {
            name: "key".to_string(),
            value: "<AMAP_KEY>".to_string(),
        },
    ]
}

/// Query 形态注入的 endpoint 拼接：追加用 ?、续接用 &、值经 percent-encoding。
#[test]
fn streamable_http_endpoint_appends_and_encodes_query_credentials() {
    use super::streamable_http_endpoint;

    // 无注入：URL 原样返回
    assert_eq!(
        streamable_http_endpoint("srv", "https://mcp.example.invalid/mcp", &[]).unwrap(),
        "https://mcp.example.invalid/mcp"
    );
    // 追加形态：无既有 query 用 ?
    assert_eq!(
        streamable_http_endpoint(
            "srv",
            "https://mcp.example.invalid/mcp",
            &[("key".to_string(), "<AMAP_KEY>".to_string())]
        )
        .unwrap(),
        "https://mcp.example.invalid/mcp?key=%3CAMAP_KEY%3E"
    );
    // 续接形态：URL 已含 query 用 &
    assert_eq!(
        streamable_http_endpoint(
            "srv",
            "https://mcp.example.invalid/mcp?lang=zh",
            &[("key".to_string(), "<AMAP_KEY>".to_string())]
        )
        .unwrap(),
        "https://mcp.example.invalid/mcp?lang=zh&key=%3CAMAP_KEY%3E"
    );
    // 值中保留字符（空格/&/=）必须被编码，避免凭据值破坏 URL 结构
    assert_eq!(
        streamable_http_endpoint(
            "srv",
            "https://mcp.example.invalid/mcp",
            &[("key".to_string(), "a b&c=d".to_string())]
        )
        .unwrap(),
        "https://mcp.example.invalid/mcp?key=a+b%26c%3Dd"
    );
}

/// Header 形态注入的请求头构造：合法名值成对；非法名/非法值报错且回显零凭据值。
#[test]
fn streamable_http_custom_headers_build_and_rejects_invalid_input() {
    use super::streamable_http_custom_headers;

    let headers = streamable_http_custom_headers(
        "srv",
        &[("Authorization".to_string(), "Bearer <AMAP_KEY>".to_string())],
    )
    .expect("valid header injection should build");
    assert_eq!(headers.len(), 1);

    let err = streamable_http_custom_headers(
        "srv",
        &[("bad header".to_string(), "Bearer <AMAP_KEY>".to_string())],
    )
    .expect_err("invalid header name must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("credential header name is invalid"),
        "{message}"
    );
    assert!(!message.contains("<AMAP_KEY>"), "{message}");

    let err = streamable_http_custom_headers(
        "srv",
        &[("Authorization".to_string(), "bad\nvalue".to_string())],
    )
    .expect_err("invalid header value must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("credential header value is invalid"),
        "{message}"
    );
    assert!(!message.contains("bad\nvalue"), "{message}");
}

/// Stdio 侧只消费 Env 形态注入：Header/Query 形态被过滤兜底（check() 已按 transport 限定）。
#[test]
fn env_injection_pairs_filters_env_only_for_stdio() {
    use super::env_injection_pairs;

    let pairs = env_injection_pairs(&[
        McpCredentialInjection::Env {
            name: "LEDGER_CREDENTIAL".to_string(),
            value: "env-secret".to_string(),
        },
        McpCredentialInjection::Header {
            name: "Authorization".to_string(),
            value: "Bearer x".to_string(),
        },
        McpCredentialInjection::Query {
            name: "key".to_string(),
            value: "q-secret".to_string(),
        },
    ]);
    assert_eq!(
        pairs,
        vec![("LEDGER_CREDENTIAL".to_string(), "env-secret".to_string())]
    );
}

/// StreamableHttp 无 url 配置：连接前置校验 fail-fast。
#[tokio::test]
async fn streamable_http_runtime_fails_fast_without_server_url() {
    let runtime = McpClientRuntime::default();
    let server = streamable_http_server_with_url(None);

    let error = runtime
        .list_tools(&server, &[])
        .await
        .expect_err("StreamableHttp server without url must fail fast");
    assert!(error.to_string().contains("url is required"), "{error}");
}

/// StreamableHttp 非对象参数：连接前置校验（不产生网络副作用）。
#[tokio::test]
async fn streamable_http_runtime_rejects_non_object_args_before_connecting() {
    let runtime = McpClientRuntime::default();
    let server =
        streamable_http_server_with_url(Some("https://mcp.example.invalid/mcp".to_string()));

    let error = runtime
        .call_tool(&server, "echo", json!("not-an-object"), &[])
        .await
        .expect_err("non-object args must be rejected before connecting");
    assert!(
        error
            .to_string()
            .contains("MCP tool arguments must be a JSON object"),
        "{error}"
    );
}

/// StreamableHttp 不可达端点（tools/call）：失败路径安全 + 错误消息零凭据值。
#[tokio::test]
async fn streamable_http_call_fails_safely_on_unreachable_endpoint() {
    let runtime = McpClientRuntime::default();
    // 127.0.0.1:9（discard 端口）连接拒绝：错误消息只含 server id，零 URL/注入值回显
    let server = streamable_http_server_with_url(Some("http://127.0.0.1:9/mcp".to_string()));

    let error = runtime
        .call_tool(
            &server,
            "echo",
            json!({ "text": "hello" }),
            &http_credential_injections(),
        )
        .await
        .expect_err("unreachable endpoint must fail safely");
    let message = error.to_string();
    assert!(
        message
            .contains("failed to initialize MCP streamable HTTP server http-echo-server session"),
        "{message}"
    );
    assert!(!message.contains("<AMAP_KEY>"), "{message}");
    assert!(!message.contains("127.0.0.1:9"), "{message}");
}

/// StreamableHttp 不可达端点（tools/list）：失败路径安全 + 错误消息零凭据值。
#[tokio::test]
async fn streamable_http_list_fails_safely_on_unreachable_endpoint() {
    let runtime = McpClientRuntime::default();
    let server = streamable_http_server_with_url(Some("http://127.0.0.1:9/mcp".to_string()));

    let error = runtime
        .list_tools(&server, &http_credential_injections())
        .await
        .expect_err("unreachable endpoint must fail safely");
    let message = error.to_string();
    assert!(
        message
            .contains("failed to initialize MCP streamable HTTP server http-echo-server session"),
        "{message}"
    );
    assert!(!message.contains("<AMAP_KEY>"), "{message}");
    assert!(!message.contains("127.0.0.1:9"), "{message}");
}

/// 本地 mock MCP server 端到端会话：完整 initialize→tools/list→tools/call 链路，
/// Header/Query 注入被服务端实证收到，会话用后即关（per-call 短会话语义）。
#[tokio::test]
async fn streamable_http_end_to_end_mock_server_header_and_query_injection() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let script = r#"
import json
import socket
from http.server import BaseHTTPRequestHandler, HTTPServer

sock = socket.socket()
sock.bind(("127.0.0.1", 0))
PORT = sock.getsockname()[1]
sock.close()
print(PORT, flush=True)

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length)
        try:
            message = json.loads(body)
        except Exception:
            self.send_response(400)
            self.end_headers()
            return
        method = message.get("method", "")
        request_id = message.get("id")
        if method.startswith("notifications/"):
            self.send_response(202)
            self.end_headers()
            return
        if method == "initialize":
            result = {
                "protocolVersion": "2025-11-25",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "http-mock", "version": "1.0.0"},
            }
        elif method == "tools/list":
            result = {
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
            }
        elif method == "tools/call":
            args = message.get("params", {}).get("arguments", {})
            text = args.get("text", "")
            result = {
                "content": [{"type": "text", "text": text}],
                "structuredContent": {
                    "echo": text,
                    "auth_seen": self.headers.get("Authorization", ""),
                    "key_seen": self.path.split("key=")[-1].split("&")[0],
                },
                "isError": False,
            }
        else:
            self.send_response(400)
            self.end_headers()
            return
        resp = json.dumps({"jsonrpc": "2.0", "id": request_id, "result": result}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(resp)))
        self.end_headers()
        self.wfile.write(resp)

    def log_message(self, *args):
        pass

HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
"#;

    let mut script_file =
        tempfile::NamedTempFile::new().expect("temp mock script should be created");
    std::io::Write::write_all(&mut script_file, script.as_bytes())
        .expect("temp mock script should be written");

    let mut child = Command::new("python3")
        .arg(script_file.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("python3 should be available for the mock MCP server");
    let mock_pid = child.id();
    let port_line = BufReader::new(child.stdout.take().expect("mock stdout"))
        .lines()
        .next()
        .expect("mock server should print its port")
        .expect("mock server port line should be readable");
    let port: u16 = port_line
        .trim()
        .parse()
        .expect("mock port should be numeric");

    let runtime = McpClientRuntime::default();
    let server = streamable_http_server_with_url(Some(format!("http://127.0.0.1:{port}/mcp")));

    // tools/list：完整会话 + 远端工具发现
    let tools = runtime
        .list_tools(&server, &http_credential_injections())
        .await
        .expect("mock StreamableHttp session should list tools");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    assert_eq!(tools[0].description.as_deref(), Some("Echo input text"));
    assert_eq!(tools[0].input_schema["type"], "object");

    // tools/call：注入实证——服务端回显收到的 Authorization 头与 key 查询参数
    let result = runtime
        .call_tool(
            &server,
            "echo",
            json!({ "text": "hello-http" }),
            &http_credential_injections(),
        )
        .await
        .expect("mock StreamableHttp session should call the tool");
    assert_eq!(result["structuredContent"]["echo"], "hello-http");
    assert_eq!(result["isError"], false);
    assert_eq!(
        result["structuredContent"]["auth_seen"], "Bearer <AMAP_KEY>",
        "Authorization header injection must reach the server on every request"
    );
    assert_eq!(
        result["structuredContent"]["key_seen"], "%3CAMAP_KEY%3E",
        "query credential must be percent-encoded on the endpoint URL"
    );

    // mock 进程显式回收（std::process::Command 无 kill_on_drop；防残留进程占用端口）
    let _ = child.kill();
    let _ = child.wait();
    let _ = mock_pid;
}
