use crate::models::mcp_server::{
    McpServer, McpServerConfig, McpServerStatus, McpTransport, REDACTED_CONFIG_VALUE,
};
use common::models::{CredentialBinding, CredentialKind, CredentialRequirement};

use super::response::to_detail;

fn redacted_stdio_server() -> McpServer {
    let config = McpServerConfig {
        command: Some("npx".to_string()),
        args: vec!["-y".to_string(), "server".to_string()],
        url: Some(format!(
            "https://api.example.com/mcp?{}",
            REDACTED_CONFIG_VALUE
        )),
        credential_requirements: vec![CredentialRequirement {
            kind: CredentialKind::GenericToken,
            platform: Some("linear".to_string()),
            field: None,
            enhancer: None,
            binding: CredentialBinding::Env {
                name: "LINEAR_API_TOKEN".to_string(),
            },
        }],
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
    // requirements 为非敏感类型级声明：管理面原样透出（无明文凭据可脱敏，D14）
    assert_eq!(detail.config.credential_requirements.len(), 1);
    assert_eq!(
        detail.config.credential_requirements[0].kind,
        CredentialKind::GenericToken
    );
    assert_eq!(
        detail.config.credential_requirements[0].platform.as_deref(),
        Some("linear")
    );
    assert_eq!(
        detail.config.url.as_deref(),
        Some("https://api.example.com/mcp?[REDACTED]")
    );
    assert_eq!(detail.status, common::enums::McpServerStatus::Disabled);
}
