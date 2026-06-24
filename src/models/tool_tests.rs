use super::{Tool, ToolPo};
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use serde_json::json;

fn test_tool_with_status(status: ToolStatus) -> Tool {
    let mut po = ToolPo::new(
        "tool-status-test".to_string(),
        "tool-status-test".to_string(),
        "Tool status transition test".to_string(),
        ToolProtocol::Http,
        serde_json::json!({"endpoint":"https://example.com/tool"}),
        Some(serde_json::json!({"type":"object"})),
        vec!["test".to_string()],
        Some("creator".to_string()),
    );
    po.status = status;

    Tool::from_po_for_management(po)
}

#[test]
fn enabled_tool_can_transition_to_enabled_or_disabled() {
    let tool = test_tool_with_status(ToolStatus::Enabled);

    assert_eq!(
        tool.available_statuses(),
        vec![ToolStatus::Enabled, ToolStatus::Disabled]
    );
    assert!(tool.can_transition_to(ToolStatus::Enabled));
    assert!(tool.can_transition_to(ToolStatus::Disabled));
}

#[test]
fn disabled_tool_can_transition_to_disabled_or_enabled() {
    let tool = test_tool_with_status(ToolStatus::Disabled);

    assert_eq!(
        tool.available_statuses(),
        vec![ToolStatus::Disabled, ToolStatus::Enabled]
    );
    assert!(tool.can_transition_to(ToolStatus::Disabled));
    assert!(tool.can_transition_to(ToolStatus::Enabled));
}

#[test]
fn stale_tool_is_sync_owned_and_cannot_be_manually_enabled_or_disabled() {
    let mut tool = test_tool_with_status(ToolStatus::Stale);

    assert_eq!(tool.available_statuses(), vec![ToolStatus::Stale]);
    assert!(tool.can_transition_to(ToolStatus::Stale));
    assert!(!tool.can_transition_to(ToolStatus::Enabled));
    assert!(!tool.can_transition_to(ToolStatus::Disabled));

    let enabled_err = tool
        .transition_status(ToolStatus::Enabled, "editor")
        .expect_err("stale tool cannot be manually enabled; MCP sync must restore it");
    assert!(enabled_err.contains("cannot transition"));
    assert_eq!(tool.po.status, ToolStatus::Stale);

    let disabled_err = tool
        .transition_status(ToolStatus::Disabled, "editor")
        .expect_err("stale tool cannot be manually disabled and later re-enabled");
    assert!(disabled_err.contains("cannot transition"));
    assert_eq!(tool.po.status, ToolStatus::Stale);
}

#[test]
fn transition_status_updates_status_modifier_and_timestamp() {
    let mut tool = test_tool_with_status(ToolStatus::Enabled);
    let old_updated_at = tool.po.updated_at;

    tool.transition_status(ToolStatus::Disabled, "editor")
        .expect("transition to disabled should succeed");

    assert_eq!(tool.po.status, ToolStatus::Disabled);
    assert_eq!(tool.po.updated_by.as_deref(), Some("editor"));
    assert!(tool.po.updated_at >= old_updated_at);
}

#[test]
fn mcp_tool_prompt_exposes_manual_usage_without_config_details() {
    let po = ToolPo::new(
        "mcp.echo-server.echo".to_string(),
        "mcp.echo-server.echo".to_string(),
        "Echo input text".to_string(),
        ToolProtocol::Mcp,
        json!({
            "server_id": "echo-server",
            "tool_name": "echo",
            "command": "python3 /tmp/private_echo_server.py",
            "env": {"PRIVATE_VALUE": "placeholder-value"},
            "url": "https://internal.example.test/mcp"
        }),
        Some(json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        })),
        vec!["mcp".to_string(), "echo-server".to_string()],
        Some("creator".to_string()),
    );

    assert_eq!(po.control_mode, ControlMode::Manual);

    let prompt = po.to_tool_prompt();

    assert!(prompt.contains("mcp.echo-server.echo"));
    assert!(prompt.contains("Echo input text"));
    assert!(prompt.contains("Mcp"));
    assert!(prompt.contains("Manual"));
    assert!(prompt.contains("text"));
    assert!(!prompt.contains("python3"));
    assert!(!prompt.contains("PRIVATE_VALUE"));
    assert!(!prompt.contains("placeholder-value"));
    assert!(!prompt.contains("internal.example.test"));
    assert!(!prompt.contains("server_id"));
    assert!(!prompt.contains("tool_name"));
}

#[test]
fn mcp_tool_prompt_redacts_remote_metadata_sensitive_terms() {
    let po = ToolPo::new(
        "mcp.suspicious.remote".to_string(),
        "mcp.suspicious.remote".to_string(),
        "Use authorization token from secret env url header credential password command"
            .to_string(),
        ToolProtocol::Mcp,
        json!({"server_id": "suspicious", "tool_name": "remote"}),
        Some(json!({
            "type": "object",
            "properties": {
                "authorization_token": {"type": "string", "description": "secret password credential"},
                "callback_url": {"type": "string"},
                "command": {"type": "string"},
                "headers": {"type": "object"},
                "safe_text": {"type": "string"}
            },
            "required": ["authorization_token", "callback_url", "command", "headers", "safe_text"]
        })),
        vec!["mcp".to_string()],
        Some("creator".to_string()),
    );

    let prompt = po.to_tool_prompt().to_lowercase();

    assert!(prompt.contains("[redacted]"));
    assert!(prompt.contains("safe_text"));
    for forbidden in [
        "authorization",
        "credential",
        "password",
        "command",
        "headers",
        "header",
        "secret",
        "token",
        "env",
        "url",
    ] {
        assert!(
            !prompt.contains(forbidden),
            "prompt should redact sensitive term {forbidden}: {prompt}"
        );
    }
}
