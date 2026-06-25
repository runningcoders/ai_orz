use std::collections::BTreeMap;

use crate::models::mcp_server::{
    McpServer, McpServerConfig, McpServerStatus, McpTransport, REDACTED_CONFIG_VALUE,
};

use super::response::to_detail;
use common::bail_err;

fn redacted_stdio_server() -> McpServer {
    let mut env = BTreeMap::new();
    env.insert("API_TOKEN".to_string(), REDACTED_CONFIG_VALUE.to_string());
    let config = McpServerConfig {
        command: Some("npx".to_string()),
        args: vec!["-y".to_string(), "server".to_string()],
        env,
        url: Some(format!(
            "https://api.example.com/mcp?{}",
            REDACTED_CONFIG_VALUE
        )),
        headers: BTreeMap::new(),
        timeout_ms: 30_000,
        connect_timeout_ms: 10_000,
        response_max_bytes: 1024,
    };
    let mut server = McpServer::new(
        "server-1".to_string(),
        "filesystem".to_string(),
        McpTransport::Stdio,
        config,
        Some("creator".to_string()),
    );
    server.po.status = McpServerStatus::Disabled;
    server
}

#[test]
fn mcp_server_detail_preserves_management_redaction_shape() {
    let server = redacted_stdio_server();

    let detail = to_detail(&server);

    assert_eq!(detail.id, "server-1");
    assert_eq!(detail.config.command.as_deref(), Some("npx"));
    assert_eq!(
        detail.config.env.get("API_TOKEN").map(String::as_str),
        Some(REDACTED_CONFIG_VALUE)
    );
    assert_eq!(
        detail.config.url.as_deref(),
        Some("https://api.example.com/mcp?[REDACTED]")
    );
    assert_eq!(detail.status, common::enums::McpServerStatus::Disabled);
}
