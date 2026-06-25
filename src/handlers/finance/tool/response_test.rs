use common::enums::ToolProtocol;
use serde_json::json;

use crate::models::tool::{Tool, ToolPo};

use super::response::to_detail;
use common::bail_err;

fn http_tool_with_config(config: serde_json::Value) -> Tool {
    let po = ToolPo::new(
        String::new(),
        "test-http-tool".to_string(),
        "test http tool".to_string(),
        ToolProtocol::Http,
        config,
        None,
        vec![],
        Some("test-user".to_string()),
    );
    Tool::from_po_for_management(po)
}

#[test]
fn tool_detail_returns_redacted_config_without_raw_sensitive_values() {
    let tool = http_tool_with_config(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "headers": {
            "Accept": "application/json",
            "Authorization": "Bearer placeholder-value",
            "X-Api-Key": "placeholder-value"
        },
        "query": {
            "keyword": "rust",
            "access_token": "placeholder-value",
            "access_key": "placeholder-value",
            "client_key": "placeholder-value"
        },
        "body": {
            "nested": {
                "password": "placeholder-value"
            }
        }
    }));

    let detail = to_detail(&tool);
    let config = detail
        .config
        .expect("detail should include redacted config");
    let text = config.to_string();

    assert!(detail.has_config);
    assert!(text.contains("[REDACTED]"));
    assert!(text.contains("api.example.com"));
    assert!(!text.contains("application/json"));
    assert!(!text.contains("placeholder-value"));
    assert_eq!(config["headers"]["Accept"], "[REDACTED]");
    assert_eq!(config["headers"]["Authorization"], "[REDACTED]");
    assert_eq!(config["headers"]["X-Api-Key"], "[REDACTED]");
    assert_eq!(config["query"]["keyword"], "[REDACTED]");
    assert_eq!(config["query"]["access_token"], "[REDACTED]");
    assert_eq!(config["query"]["access_key"], "[REDACTED]");
    assert_eq!(config["query"]["client_key"], "[REDACTED]");
    assert_eq!(config["body"]["nested"]["password"], "[REDACTED]");
}

#[test]
fn tool_detail_redacts_url_userinfo_and_sensitive_query_values() {
    let tool = http_tool_with_config(json!({
        "method": "GET",
        "url": "https://user:placeholder-value@api.example.com/search?q=rust&access_token=placeholder-value&debug=true"
    }));

    let detail = to_detail(&tool);
    let config = detail
        .config
        .expect("detail should include redacted config");
    let url = config["url"]
        .as_str()
        .expect("redacted URL should remain a string");

    assert!(url.contains("api.example.com"));
    assert!(url.contains("q=%5BREDACTED%5D") || url.contains("q=[REDACTED]"));
    assert!(url.contains("debug=%5BREDACTED%5D") || url.contains("debug=[REDACTED]"));
    assert!(url.contains("access_token=%5BREDACTED%5D") || url.contains("access_token=[REDACTED]"));
    assert!(!url.contains("placeholder-value"));
    assert!(!url.contains("user:"));
}
