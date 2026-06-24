//! MCP Tool DAL skeleton contract tests.
//!
//! Stage 3 verifies MCP-specific DAL orchestration: read ToolPo, read
//! McpServerPo, then ask McpToolCallDao for an executable MCP CoreTool.

use crate::error::Result;
use crate::models::mcp_server::{McpServerConfig, McpServerPo, McpTransport};
use crate::models::tool::{Tool, ToolPo};
use crate::pkg::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallStatus;
use crate::pkg::tool_tracing::logger::ToolCallLogger;
use crate::service::dal::mcp_tool::{self, McpToolDal};
use crate::service::dao::{mcp_server, tool, tool_call};
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::{Arc, Once};

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

fn mcp_server_with_command(id: &str, command: String, args: Vec<String>) -> McpServerPo {
    McpServerPo::new(
        id.to_string(),
        "echo".to_string(),
        McpTransport::Stdio,
        McpServerConfig {
            command: Some(command),
            args,
            ..McpServerConfig::default_stdio()
        },
        Some("test-user".to_string()),
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

fn init_test_env(pool: SqlitePool) -> (Arc<dyn McpToolDal + Send + Sync>, RequestContext) {
    init_test_tool_call_logger();
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

fn init_test_tool_call_logger() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let base_path = std::env::temp_dir().join(format!(
            "ai_orz_mcp_tool_dal_trace_tests_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&base_path).expect("test tool trace base path should be created");
        ToolCallLogger::init(base_path);
    });
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
async fn mcp_tool_dal_syncs_stdio_server_tools_into_tool_records(pool: SqlitePool) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;

    let synced = dal.sync_from_server(ctx.clone(), &server.id).await?;
    assert_eq!(synced, 1);

    let persisted = tool::new_tool_dao()
        .get_by_id(ctx.clone(), "mcp.echo-server.echo".to_string())
        .await?
        .expect("synced MCP tool should be persisted as a standard ToolPo");

    assert_eq!(persisted.id, "mcp.echo-server.echo");
    assert_eq!(persisted.name, "mcp.echo-server.echo");
    assert_eq!(persisted.description, "Echo input text");
    assert_eq!(persisted.protocol, ToolProtocol::Mcp);
    assert_eq!(persisted.control_mode, ControlMode::Manual);
    assert_eq!(
        persisted.config,
        json!({
            "server_id": "echo-server",
            "tool_name": "echo",
        })
    );
    assert_eq!(
        persisted.parameters_schema,
        Some(json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        }))
    );
    assert!(persisted.tags.contains(&"mcp".to_string()));
    assert!(persisted.tags.contains(&"echo-server".to_string()));
    assert!(persisted.tags.contains(&"echo".to_string()));
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn sync_then_call_stdio_mcp_tool_by_id_returns_result(pool: SqlitePool) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;

    let synced = dal.sync_from_server(ctx.clone(), &server.id).await?;
    assert_eq!(synced, 1);

    let tool_id = "mcp.echo-server.echo".to_string();
    let persisted = tool::new_tool_dao()
        .get_by_id(ctx.clone(), tool_id.clone())
        .await?
        .expect("synced MCP tool should be persisted before runtime call");
    assert_eq!(
        persisted.config,
        json!({
            "server_id": "echo-server",
            "tool_name": "echo",
        })
    );

    let result = dal
        .call_tool_by_id(ctx.clone(), tool_id.clone(), json!({ "text": "hello MCP" }))
        .await
        .expect("synced MCP stdio tool should execute by id");

    assert_eq!(result["structuredContent"]["echo"], "hello MCP");
    assert_eq!(result["isError"], false);

    let executable = dal
        .get_by_id(ctx.clone(), tool_id.clone())
        .await?
        .expect("synced MCP tool should be executable");
    let management_tool = Tool::from_po_for_management(executable.po.clone());
    let from_management_result = dal
        .call_tool(
            ctx.clone(),
            &management_tool,
            json!({ "text": "management MCP" }),
        )
        .await
        .expect("McpToolDal should reassemble executable MCP tool from authorized metadata");
    assert_eq!(
        from_management_result["structuredContent"]["echo"],
        "management MCP"
    );

    let (manual_result, entry) = dal
        .call_manual(ctx, &executable, json!({ "text": "manual MCP" }))
        .await
        .expect("manual MCP stdio tool call should return trace entry");

    assert_eq!(manual_result["structuredContent"]["echo"], "manual MCP");
    assert_eq!(entry.tool_id, tool_id);
    assert_eq!(entry.tool_name, "mcp.echo-server.echo");
    assert_eq!(entry.status, ToolCallStatus::Completed);
    assert_eq!(entry.input, json!("[REDACTED]"));
    assert_eq!(entry.output, Some(json!("[REDACTED]")));
    assert!(entry.error.is_none());
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_sync_upserts_existing_tool_and_preserves_audit_and_status(
    pool: SqlitePool,
) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;

    let mut existing = ToolPo::new(
        "mcp.echo-server.echo".to_string(),
        "stale-name".to_string(),
        "Old local description".to_string(),
        ToolProtocol::Mcp,
        json!({
            "server_id": "echo-server",
            "tool_name": "echo",
        }),
        Some(json!({"type": "object", "properties": {}})),
        vec!["old-tag".to_string()],
        Some("original-user".to_string()),
    );
    existing.status = ToolStatus::Disabled;
    let original_created_at = existing.created_at;
    let original_created_by = existing.created_by.clone();
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &existing)
        .await?;

    let synced = dal.sync_from_server(ctx.clone(), &server.id).await?;
    assert_eq!(synced, 1);

    let persisted = tool::new_tool_dao()
        .get_by_id(ctx.clone(), "mcp.echo-server.echo".to_string())
        .await?
        .expect("existing MCP tool should still exist after sync upsert");

    assert_eq!(persisted.name, "mcp.echo-server.echo");
    assert_eq!(persisted.description, "Echo input text");
    assert_eq!(persisted.protocol, ToolProtocol::Mcp);
    assert_eq!(persisted.control_mode, ControlMode::Manual);
    assert_eq!(persisted.status, ToolStatus::Disabled);
    assert_eq!(persisted.created_at, original_created_at);
    assert_eq!(persisted.created_by, original_created_by);
    assert_eq!(persisted.updated_by, Some("test-user".to_string()));
    assert_eq!(persisted.config["server_id"], "echo-server");
    assert_eq!(persisted.config["tool_name"], "echo");
    assert_eq!(
        persisted.parameters_schema.as_ref().unwrap()["properties"]["text"]["type"],
        "string"
    );
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_sync_marks_missing_enabled_tools_stale_and_preserves_disabled(
    pool: SqlitePool,
) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;

    let mut missing_enabled = ToolPo::new(
        "mcp.echo-server.old".to_string(),
        "mcp.echo-server.old".to_string(),
        "Remote tool that disappeared".to_string(),
        ToolProtocol::Mcp,
        json!({"server_id": "echo-server", "tool_name": "old"}),
        Some(json!({"type": "object"})),
        vec![
            "mcp".to_string(),
            "echo-server".to_string(),
            "old".to_string(),
        ],
        Some("original-user".to_string()),
    );
    missing_enabled.status = ToolStatus::Enabled;
    let mut missing_disabled = ToolPo::new(
        "mcp.echo-server.disabled".to_string(),
        "mcp.echo-server.disabled".to_string(),
        "Admin disabled disappeared tool".to_string(),
        ToolProtocol::Mcp,
        json!({"server_id": "echo-server", "tool_name": "disabled"}),
        Some(json!({"type": "object"})),
        vec![
            "mcp".to_string(),
            "echo-server".to_string(),
            "disabled".to_string(),
        ],
        Some("original-user".to_string()),
    );
    missing_disabled.status = ToolStatus::Disabled;
    let disabled_created_at = missing_disabled.created_at;

    tool::new_tool_dao()
        .create_tool(ctx.clone(), &missing_enabled)
        .await?;
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &missing_disabled)
        .await?;
    tool::new_tool_dao()
        .add_tool_to_agent(
            ctx.clone(),
            "agent-keeps-binding",
            &missing_enabled.id,
            Some("test-user".to_string()),
        )
        .await?;

    let synced = dal.sync_from_server(ctx.clone(), &server.id).await?;
    assert_eq!(synced, 1);

    let stale = tool::new_tool_dao()
        .get_by_id(ctx.clone(), "mcp.echo-server.old".to_string())
        .await?
        .expect("missing enabled MCP tool should be retained locally");
    assert_eq!(stale.status, ToolStatus::Stale);
    assert_eq!(stale.config["server_id"], "echo-server");
    assert_eq!(stale.config["tool_name"], "old");

    let disabled = tool::new_tool_dao()
        .get_by_id(ctx.clone(), "mcp.echo-server.disabled".to_string())
        .await?
        .expect("missing disabled MCP tool should be retained locally");
    assert_eq!(disabled.status, ToolStatus::Disabled);
    assert_eq!(disabled.created_at, disabled_created_at);

    let bindings = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM agent_tools WHERE agent_id = ? AND tool_id = ?",
    )
    .bind("agent-keeps-binding")
    .bind("mcp.echo-server.old")
    .fetch_one(ctx.db_pool())
    .await?;
    assert_eq!(
        bindings, 1,
        "stale reconcile must not delete Agent bindings"
    );
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_sync_restores_stale_tool_when_remote_reappears(
    pool: SqlitePool,
) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;

    let mut existing = ToolPo::new(
        "mcp.echo-server.echo".to_string(),
        "mcp.echo-server.echo".to_string(),
        "Previously stale remote tool".to_string(),
        ToolProtocol::Mcp,
        json!({"server_id": "echo-server", "tool_name": "echo"}),
        Some(json!({"type": "object"})),
        vec![
            "mcp".to_string(),
            "echo-server".to_string(),
            "echo".to_string(),
        ],
        Some("original-user".to_string()),
    );
    existing.status = ToolStatus::Stale;
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &existing)
        .await?;

    let synced = dal.sync_from_server(ctx.clone(), &server.id).await?;
    assert_eq!(synced, 1);

    let restored = tool::new_tool_dao()
        .get_by_id(ctx.clone(), "mcp.echo-server.echo".to_string())
        .await?
        .expect("reappeared stale MCP tool should still exist");
    assert_eq!(restored.status, ToolStatus::Enabled);
    assert_eq!(restored.description, "Echo input text");
    assert_eq!(
        restored.parameters_schema.as_ref().unwrap()["properties"]["text"]["type"],
        "string"
    );
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_sync_keeps_disabled_tool_disabled_when_remote_reappears(
    pool: SqlitePool,
) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;

    let mut existing = ToolPo::new(
        "mcp.echo-server.echo".to_string(),
        "mcp.echo-server.echo".to_string(),
        "Admin disabled remote tool".to_string(),
        ToolProtocol::Mcp,
        json!({"server_id": "echo-server", "tool_name": "echo"}),
        Some(json!({"type": "object"})),
        vec![
            "mcp".to_string(),
            "echo-server".to_string(),
            "echo".to_string(),
        ],
        Some("original-user".to_string()),
    );
    existing.status = ToolStatus::Disabled;
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &existing)
        .await?;

    let synced = dal.sync_from_server(ctx.clone(), &server.id).await?;
    assert_eq!(synced, 1);

    let persisted = tool::new_tool_dao()
        .get_by_id(ctx.clone(), "mcp.echo-server.echo".to_string())
        .await?
        .expect("disabled MCP tool should still exist after re-sync");
    assert_eq!(persisted.status, ToolStatus::Disabled);
    assert_eq!(persisted.description, "Echo input text");
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_sync_rejects_non_mcp_id_collision(pool: SqlitePool) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;

    let collision = ToolPo::new_builtin(
        "mcp.echo-server.echo".to_string(),
        "builtin_collision".to_string(),
        "Existing non-MCP tool with MCP-generated id".to_string(),
    );
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &collision)
        .await?;

    let err = dal
        .sync_from_server(ctx.clone(), &server.id)
        .await
        .expect_err("MCP sync should reject non-MCP id collisions instead of overwriting");

    assert!(err.to_string().contains("already exists as non-MCP tool"));
    let persisted = tool::new_tool_dao()
        .get_by_id(ctx.clone(), collision.id.clone())
        .await?
        .expect("collision tool should remain untouched");
    assert_eq!(persisted.protocol, ToolProtocol::Builtin);
    assert_eq!(persisted.name, "builtin_collision");
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_sync_rejects_mcp_binding_collision(pool: SqlitePool) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let script = write_echo_mcp_server_script();
    let server = mcp_server_with_command(
        "echo-server",
        "python3".to_string(),
        vec![script.path().to_string_lossy().to_string()],
    );

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;

    let existing = ToolPo::new(
        "mcp.echo-server.echo".to_string(),
        "wrong-binding".to_string(),
        "Existing MCP tool with mismatched binding".to_string(),
        ToolProtocol::Mcp,
        json!({
            "server_id": "other-server",
            "tool_name": "echo",
        }),
        Some(json!({"type": "object", "properties": {}})),
        vec!["mcp".to_string()],
        Some("original-user".to_string()),
    );
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &existing)
        .await?;

    let err = dal
        .sync_from_server(ctx.clone(), &server.id)
        .await
        .expect_err("MCP sync should reject mismatched MCP config bindings");

    assert!(
        err.to_string()
            .contains("already binds to other-server/echo")
    );
    let persisted = tool::new_tool_dao()
        .get_by_id(ctx.clone(), existing.id.clone())
        .await?
        .expect("existing MCP collision record should remain untouched");
    assert_eq!(persisted.name, "wrong-binding");
    assert_eq!(persisted.config["server_id"], "other-server");
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

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_rejects_disabled_tool_when_calling_by_id(pool: SqlitePool) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let server = mcp_server("disabled-tool-server");
    let mut po = mcp_tool_po(&server.id, "read_file");
    po.status = ToolStatus::Disabled;

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;
    tool::new_tool_dao().create_tool(ctx.clone(), &po).await?;

    let err = dal
        .call_tool_by_id(ctx.clone(), po.id.clone(), json!({ "path": "/tmp/a" }))
        .await
        .expect_err("McpToolDal should reject disabled MCP tool before execution");

    assert!(err.to_string().contains("MCP tool disabled"));
    assert!(err.to_string().contains(&po.id));
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_rejects_stale_tool_when_calling_by_id(pool: SqlitePool) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let server = mcp_server("stale-tool-server");
    let mut po = mcp_tool_po(&server.id, "read_file");
    po.status = ToolStatus::Stale;

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;
    tool::new_tool_dao().create_tool(ctx.clone(), &po).await?;

    let err = dal
        .call_tool_by_id(ctx.clone(), po.id.clone(), json!({ "path": "/tmp/a" }))
        .await
        .expect_err("McpToolDal should reject stale MCP tool before execution");

    assert!(err.to_string().contains("MCP tool disabled"));
    assert!(err.to_string().contains(&po.id));
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_tool_dal_rejects_disabled_server_when_calling_by_id(pool: SqlitePool) -> Result<()> {
    let (dal, ctx) = init_test_env(pool);
    let mut server = mcp_server("disabled-server");
    server.status = crate::models::mcp_server::McpServerStatus::Disabled;
    let po = mcp_tool_po(&server.id, "read_file");

    mcp_server::new_mcp_server_dao()
        .insert(ctx.clone(), &server)
        .await?;
    tool::new_tool_dao().create_tool(ctx.clone(), &po).await?;

    let err = dal
        .call_tool_by_id(ctx.clone(), po.id.clone(), json!({ "path": "/tmp/a" }))
        .await
        .expect_err("McpToolDal should reject disabled MCP server before execution");

    assert!(err.to_string().contains("MCP server disabled"));
    assert!(err.to_string().contains(&server.id));
    Ok(())
}

#[test]
fn mcp_tool_global_dal_invalidates_global_mcp_tool_call_runtime() {
    crate::service::dao::mcp_server::init();
    crate::service::dao::tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::mcp_tool::init();

    let server_id = "global-singleton-server";
    assert!(
        !tool_call::mcp_dao().is_mcp_server_invalidated(server_id),
        "fresh global MCP ToolCall DAO runtime should not already be invalidated"
    );

    mcp_tool::dal().invalidate_server(server_id);

    assert!(
        tool_call::mcp_dao().is_mcp_server_invalidated(server_id),
        "global McpToolDal must reuse tool_call::mcp_dao() so invalidation reaches the same runtime"
    );
}
