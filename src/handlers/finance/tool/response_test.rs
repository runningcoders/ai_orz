use common::api::ToolCredentialSourceKind;
use common::enums::ToolProtocol;
use common::models::{CredentialBinding, CredentialKind};
use serde_json::json;

use crate::models::tool::{Tool, ToolPo};

use super::response::to_detail;

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

#[test]
fn tool_detail_exposes_builtin_factory_credential_requirements() {
    // 测试环境全局 registry 默认未注册工厂（register_all 仅生产 init 调用），
    // 按既有测试模式：注册特定工厂 → 断言 → unregister 清理
    let registry = crate::pkg::tool_registry::get_registry();
    registry.register_builtin_factory(Box::new(
        crate::pkg::tool_registry::gh_cli::GhCliToolFactory,
    ));
    let po = ToolPo::new(
        "gh_cli".to_string(),
        "gh_cli".to_string(),
        "GitHub CLI 工具".to_string(),
        ToolProtocol::Builtin,
        json!({ "command": "gh" }),
        None,
        vec![],
        Some("system".to_string()),
    );
    let tool = Tool::from_po_for_management(po);

    let detail = to_detail(&tool);

    assert_eq!(detail.credential_requirements.len(), 1);
    assert_eq!(
        detail.credential_requirements[0].kind,
        CredentialKind::GithubToken
    );
    assert_eq!(
        detail.credential_requirements[0].binding,
        CredentialBinding::Internal {
            field: "token".to_string()
        }
    );
    registry.unregister("gh_cli");
}

#[test]
fn tool_detail_exposes_config_declared_credential_requirements_for_http_tool() {
    let tool = http_tool_with_config(json!({
        "url": "https://api.example.com/v1",
        "credential_requirements": [
            {
                "kind": "github_token",
                "binding": { "type": "internal", "field": "token" }
            }
        ]
    }));

    let detail = to_detail(&tool);

    // 顶层字段从原始 config 聚合解析（registry 统一入口），非敏感直接展示
    assert_eq!(detail.credential_requirements.len(), 1);
    assert_eq!(
        detail.credential_requirements[0].kind,
        CredentialKind::GithubToken
    );
    assert_eq!(
        detail.credential_requirements[0].binding,
        CredentialBinding::Internal {
            field: "token".to_string()
        }
    );
    // config 通道内同名键仍走脱敏（这正是需要独立顶层字段的原因）
    assert_eq!(
        detail.config.unwrap()["credential_requirements"],
        "[REDACTED]"
    );
}

fn mcp_tool_with_config(config: serde_json::Value) -> Tool {
    let po = ToolPo::new(
        String::new(),
        "test-mcp-tool".to_string(),
        "test mcp tool".to_string(),
        ToolProtocol::Mcp,
        config,
        None,
        vec![],
        Some("test-user".to_string()),
    );
    Tool::from_po_for_management(po)
}

#[test]
fn mcp_tool_credential_source_only_reports_server_identity() {
    let tool = mcp_tool_with_config(json!({
        "server_id": "srv-42",
        "tool_name": "remote.echo"
    }));
    let source = super::response::credential_source(&tool, Some("生产 MCP".to_string()))
        .expect("MCP 工具应回显凭据继承来源");
    assert_eq!(source.kind, ToolCredentialSourceKind::McpServer);
    assert_eq!(source.id, "srv-42");
    assert_eq!(source.name, "生产 MCP");

    // 关键：详情仍不复制 Server 的声明内容，工具行自身声明恒空；
    // 来源标识由 handler 经 attach_credential_source 异步补齐。
    let detail = to_detail(&tool);
    assert!(detail.credential_requirements.is_empty());
    assert!(detail.credential_source.is_none());
    // server_id 不属于敏感键，config 回显里仍明文可见（前端可据此兜底）
    assert_eq!(detail.config.unwrap()["server_id"], "srv-42");
}

#[test]
fn credential_source_falls_back_to_id_and_skips_non_mcp_or_broken_config() {
    let tool = mcp_tool_with_config(json!({
        "server_id": "srv-42",
        "tool_name": "remote.echo"
    }));
    assert_eq!(
        super::response::credential_source(&tool, None)
            .unwrap()
            .name,
        "srv-42"
    );
    assert_eq!(
        super::response::credential_source(&tool, Some(String::new()))
            .unwrap()
            .name,
        "srv-42"
    );

    // 非 MCP 协议不标注继承来源（它有自己的声明）
    let http = http_tool_with_config(json!({}));
    assert!(super::response::credential_source(&http, None).is_none());

    // 畸形工具行 config 安静跳过，详情接口不因 Server 数据异常而失败
    let broken = mcp_tool_with_config(json!({ "tool_name": "remote.echo" }));
    assert!(super::response::credential_source(&broken, None).is_none());
}
