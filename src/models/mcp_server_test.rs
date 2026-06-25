use super::{McpServerConfig, REDACTED_CONFIG_VALUE};
use common::bail_err;

#[test]
fn redacted_for_management_redacts_url_userinfo_and_query() {
    let config = McpServerConfig {
        url: Some("https://user:password@api.example.com/mcp?token=secret#section".to_string()),
        ..McpServerConfig::default_streamable_http()
    };

    let redacted = config.redacted_for_management();

    assert_eq!(
        redacted.url.as_deref(),
        Some("https://[REDACTED]@api.example.com/mcp?[REDACTED]#section")
    );
    let redacted_url = redacted.url.as_deref().unwrap_or_default();
    assert!(!redacted_url.contains("password"));
    assert!(!redacted_url.contains("secret"));
}
