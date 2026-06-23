use super::{McpToolConfig, create_tool};
use crate::models::tool::ToolPo;
use crate::pkg::tool_registry::ToolRegistry;
use common::enums::tool::ControlMode;
use common::enums::{ToolProtocol, ToolStatus};
use serde_json::json;

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
