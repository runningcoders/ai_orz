use super::HttpToolConfig;
use crate::models::tool::ToolPo;
use crate::pkg::tool_registry::ToolRegistry;
use common::enums::tool::ControlMode;
use common::enums::{ToolProtocol, ToolStatus};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn http_tool_po(config: serde_json::Value) -> ToolPo {
    let mut po = ToolPo::new(
        "github_search_repositories".to_string(),
        "github_search_repositories".to_string(),
        "Search GitHub repositories by query".to_string(),
        ToolProtocol::Http,
        config,
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["query"]
        })),
        vec!["github".to_string(), "search".to_string()],
        Some("test".to_string()),
    );
    po.status = ToolStatus::Enabled;
    po
}

#[test]
fn http_tool_po_defaults_to_manual_control_mode() {
    let po = ToolPo::new(
        String::new(),
        "default-manual-http-tool".to_string(),
        "HTTP tool should default to manual control mode".to_string(),
        ToolProtocol::Http,
        json!({
            "method": "GET",
            "url": "https://api.example.com/search"
        }),
        None,
        Vec::new(),
        Some("test".to_string()),
    );

    assert_eq!(po.control_mode, ControlMode::Manual);
}

#[test]
fn http_tool_config_roundtrips_from_tool_po_config() {
    let config_json = json!({
        "method": "GET",
        "url": "https://api.github.com/search/repositories",
        "headers": {
            "Accept": "application/vnd.github+json"
        },
        "query": {
            "q": "{{args.query}}",
            "per_page": "{{args.limit}}"
        },
        "body": null,
        "timeout_ms": 10_000,
        "response_max_bytes": 65_536,
        "allowed_status_codes": [200],
        "response_json_pointer": "/items",
        "allowed_domains": ["api.github.com"],
        "blocked_domains": ["localhost", "127.0.0.1"],
        "allow_local_network": false
    });

    let po = http_tool_po(config_json);
    let config: HttpToolConfig = serde_json::from_value(po.config.clone())
        .expect("ToolPo.config should deserialize into HttpToolConfig");

    assert_eq!(config.method, "GET");
    assert_eq!(config.url, "https://api.github.com/search/repositories");
    assert_eq!(config.timeout_ms, Some(10_000));
    assert_eq!(config.response_max_bytes, Some(65_536));
    assert_eq!(config.allowed_status_codes, Some(vec![200]));
    assert_eq!(config.response_json_pointer.as_deref(), Some("/items"));
    assert_eq!(
        config.allowed_domains,
        Some(vec!["api.github.com".to_string()])
    );
    assert_eq!(
        config.blocked_domains,
        Some(vec!["localhost".to_string(), "127.0.0.1".to_string()])
    );
    assert_eq!(config.allow_local_network, Some(false));
    assert_eq!(
        config.query.expect("query template should exist")["q"],
        "{{args.query}}"
    );
}

#[test]
fn registry_creates_manual_http_core_tool_from_tool_po_config() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.github.com/search/repositories",
        "query": { "q": "{{args.query}}" },
        "timeout_ms": 5_000,
        "response_max_bytes": 16_384,
        "allowed_domains": ["api.github.com"]
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    assert_eq!(tool.po().protocol, ToolProtocol::Http);
    assert_eq!(tool.po().control_mode, ControlMode::Manual);
    assert_eq!(tool.po().name, "github_search_repositories");
}

#[test]
fn registry_uses_injected_http_protocol_factory() {
    use crate::models::tool::CoreTool;
    use crate::pkg::tool_registry::HttpToolFactory;
    use async_trait::async_trait;
    use common::error::Result;
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct DummyHttpTool {
        po: ToolPo,
    }

    #[async_trait]
    impl CoreTool for DummyHttpTool {
        async fn call(&self, _ctx: crate::pkg::RequestContext, _args: Value) -> Result<Value> {
            Ok(json!({ "ok": true }))
        }

        fn po(&self) -> &ToolPo {
            &self.po
        }
    }

    struct RecordingHttpFactory {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl HttpToolFactory for RecordingHttpFactory {
        fn create(&self, po: ToolPo) -> Result<Box<dyn CoreTool>> {
            self.calls.lock().unwrap().push(po.id.clone());
            Ok(Box::new(DummyHttpTool { po }))
        }
    }

    let calls = Arc::new(Mutex::new(Vec::new()));
    let registry = ToolRegistry::with_http_factory(Arc::new(RecordingHttpFactory {
        calls: calls.clone(),
    }));
    let po = http_tool_po(json!({
        "method": "GET"
    }));

    let tool = registry
        .create_tool(po)
        .expect("registry should delegate HTTP construction to injected factory");

    assert_eq!(tool.po().protocol, ToolProtocol::Http);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &["github_search_repositories".to_string()]
    );
}

#[test]
fn registry_rejects_invalid_http_tool_config() {
    let po = http_tool_po(json!({
        "method": "GET"
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "missing required http url should not create an executable tool"
    );
}

#[test]
fn registry_rejects_unsupported_http_method() {
    let po = http_tool_po(json!({
        "method": "DELETE",
        "url": "https://api.github.com/repos/owner/repo",
        "allowed_domains": ["api.github.com"]
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "HTTP runtime should only support explicitly reviewed GET/POST methods for now"
    );
}

#[test]
fn registry_rejects_unsupported_http_url_scheme_at_config_time() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "ftp://api.example.com/search"
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "HTTP config validation should reject non-http(s) URL schemes before persistence/runtime"
    );
}

#[test]
fn registry_rejects_http_url_userinfo_at_config_time() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "https://user:placeholder-value@api.example.com/search"
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "HTTP config validation should reject URL userinfo because it can contain credentials"
    );
}

#[test]
fn registry_rejects_invalid_http_timeout_bounds() {
    let registry = ToolRegistry::default();

    let zero_timeout = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "timeout_ms": 0
    }));
    assert!(
        registry.create_tool(zero_timeout).is_none(),
        "HTTP config validation should reject zero timeout"
    );

    let huge_timeout = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "timeout_ms": 600_001
    }));
    assert!(
        registry.create_tool(huge_timeout).is_none(),
        "HTTP config validation should reject timeout above hard limit"
    );
}

#[test]
fn registry_rejects_malformed_fixed_url_and_invalid_header_names() {
    let registry = ToolRegistry::default();

    let malformed_url = http_tool_po(json!({
        "method": "GET",
        "url": "https://api example.com/search"
    }));
    assert!(
        registry.create_tool(malformed_url).is_none(),
        "HTTP config validation should reject malformed fixed URLs before persistence/runtime"
    );

    let invalid_header_name = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "headers": {
            "bad header": "value"
        }
    }));
    assert!(
        registry.create_tool(invalid_header_name).is_none(),
        "HTTP config validation should reject invalid fixed header names before persistence/runtime"
    );
}

#[test]
fn registry_rejects_local_blocked_and_disallowed_fixed_targets_at_config_time() {
    let registry = ToolRegistry::default();

    let localhost = http_tool_po(json!({
        "method": "GET",
        "url": "http://localhost:1/search"
    }));
    assert!(
        registry.create_tool(localhost).is_none(),
        "HTTP config validation should reject localhost targets before persistence/runtime unless explicitly allowed"
    );

    let loopback_ip = http_tool_po(json!({
        "method": "GET",
        "url": "http://127.0.0.1:1/search"
    }));
    assert!(
        registry.create_tool(loopback_ip).is_none(),
        "HTTP config validation should reject loopback IP targets before persistence/runtime unless explicitly allowed"
    );

    let blocked_host = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "blocked_domains": ["example.com"]
    }));
    assert!(
        registry.create_tool(blocked_host).is_none(),
        "HTTP config validation should reject fixed hosts matched by blocked_domains"
    );

    let disallowed_host = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "allowed_domains": ["api.allowed.example"]
    }));
    assert!(
        registry.create_tool(disallowed_host).is_none(),
        "HTTP config validation should reject fixed hosts outside allowed_domains"
    );

    let malformed_authority_with_path_placeholder = http_tool_po(json!({
        "method": "GET",
        "url": "https://api example.com/search/{{args.query}}"
    }));
    assert!(
        registry
            .create_tool(malformed_authority_with_path_placeholder)
            .is_none(),
        "HTTP config validation should parse fixed scheme/authority even when path/query placeholders exist"
    );
}

#[test]
fn registry_rejects_whitespace_template_placeholder_form() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search/{{ args.query }}"
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "HTTP config validation should reject placeholder forms that runtime will not render"
    );
}

#[test]
fn registry_rejects_invalid_http_body_status_and_pointer_config() {
    let registry = ToolRegistry::default();

    let unsupported_body_placeholder = http_tool_po(json!({
        "method": "POST",
        "url": "https://api.example.com/search",
        "body": {
            "value": "{{runtime.value}}"
        }
    }));
    assert!(
        registry.create_tool(unsupported_body_placeholder).is_none(),
        "HTTP config validation should reject unsupported body placeholders before persistence/runtime"
    );

    let empty_status_codes = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "allowed_status_codes": []
    }));
    assert!(
        registry.create_tool(empty_status_codes).is_none(),
        "HTTP config validation should reject empty allowed_status_codes"
    );

    let invalid_status_code = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "allowed_status_codes": [99]
    }));
    assert!(
        registry.create_tool(invalid_status_code).is_none(),
        "HTTP config validation should reject invalid HTTP status codes"
    );

    let invalid_json_pointer = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "response_json_pointer": "items/0"
    }));
    assert!(
        registry.create_tool(invalid_json_pointer).is_none(),
        "HTTP config validation should reject invalid response_json_pointer"
    );
}

#[test]
fn registry_rejects_invalid_http_header_and_query_template_shapes() {
    let registry = ToolRegistry::default();

    let non_object_headers = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "headers": "not-an-object"
    }));
    assert!(
        registry.create_tool(non_object_headers).is_none(),
        "HTTP config validation should reject non-object headers"
    );

    let non_scalar_query = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "query": {
            "filter": { "token": "placeholder-value" }
        }
    }));
    assert!(
        registry.create_tool(non_scalar_query).is_none(),
        "HTTP config validation should reject non-scalar query values before runtime"
    );

    let templated_header_name = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.example.com/search",
        "headers": {
            "X-{{args.query}}": "value"
        }
    }));
    assert!(
        registry.create_tool(templated_header_name).is_none(),
        "HTTP config validation should reject template placeholders in header/query keys"
    );
}

#[test]
fn registry_rejects_http_response_max_bytes_over_hard_limit() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.github.com/search/repositories",
        "response_max_bytes": 10 * 1024 * 1024 + 1,
        "allowed_domains": ["api.github.com"]
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "HTTP runtime should reject response_max_bytes above the global hard limit"
    );
}

#[test]
fn registry_rejects_http_response_max_bytes_zero() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "https://api.github.com/search/repositories",
        "response_max_bytes": 0,
        "allowed_domains": ["api.github.com"]
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "HTTP runtime should reject zero response_max_bytes"
    );
}

#[test]
fn registry_rejects_url_template_placeholders_in_authority() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "https://{{args.host}}/search",
        "response_max_bytes": 4_096
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "HTTP tool URL authority must stay fixed and must not be controlled by model args"
    );
}

#[test]
fn registry_rejects_blocked_domain_before_network_request() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://localhost:1/search",
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096,
        "blocked_domains": ["localhost"],
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "blocked fixed domain should be rejected before persistence/runtime"
    );
}

#[test]
fn registry_rejects_local_network_by_default() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://127.0.0.1:1/search",
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "literal local network targets should require explicit allow_local_network authorization before persistence/runtime"
    );
}

#[test]
fn registry_rejects_ipv4_mapped_ipv6_local_network_by_default() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://[::ffff:127.0.0.1]:1/search",
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "IPv4-mapped IPv6 localhost should be rejected before persistence/runtime"
    );
}

#[test]
fn registry_rejects_ipv6_transition_addresses_by_default() {
    for url in [
        "http://[64:ff9b::c000:201]:1/search",
        "http://[2002:c000:0201::1]:1/search",
        "http://[2001:0000:4136:e378:8000:63bf:3fff:fdd2]:1/search",
    ] {
        let po = http_tool_po(json!({
            "method": "GET",
            "url": url,
            "timeout_ms": 1_000,
            "response_max_bytes": 4_096
        }));

        let registry = ToolRegistry::default();
        assert!(
            registry.create_tool(po).is_none(),
            "IPv6 transition address {url} should require explicit allow_local_network before persistence/runtime"
        );
    }
}

#[tokio::test]
async fn http_core_tool_rejects_domain_that_resolves_to_local_network_by_default() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://localhost.localdomain:1/search",
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let error = tool
        .call(ctx, json!({ "query": "rust" }))
        .await
        .expect_err("domain resolving to local network should require explicit allow_local_network authorization");

    let message = error.to_string();
    assert!(
        message.contains("resolved local network http target requires allow_local_network=true"),
        "unexpected error message: {message}"
    );
}

#[test]
fn registry_rejects_shared_address_space_by_default() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://100.64.0.1:1/search",
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "shared address space should be rejected before persistence/runtime"
    );
}

#[test]
fn registry_rejects_blocked_domain_with_trailing_dot() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://localhost.:1/search",
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096,
        "blocked_domains": ["localhost"],
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "blocked domain should be canonicalized before matching at config time"
    );
}

#[tokio::test]
async fn http_core_tool_rejects_schema_type_mismatch_before_network_request() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://127.0.0.1:1/search",
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096,
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let error = tool
        .call(ctx, json!({ "query": "rust", "limit": "3" }))
        .await
        .expect_err("schema type mismatch should be rejected before HTTP request");

    let message = error.to_string();
    assert!(
        message.contains("invalid type for tool argument limit"),
        "unexpected error message: {message}"
    );
}

#[tokio::test]
async fn http_core_tool_rejects_unresolved_template_placeholder() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://127.0.0.1:1/search/{{args.missing}}",
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096,
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let error = tool
        .call(ctx, json!({ "query": "rust" }))
        .await
        .expect_err("unresolved template placeholders should be rejected before HTTP request");

    let message = error.to_string();
    assert!(
        message.contains("unresolved or unsupported http template placeholder"),
        "unexpected error message: {message}"
    );
}

#[test]
fn registry_rejects_unsupported_template_placeholder_at_config_time() {
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://127.0.0.1:1/search",
        "allow_local_network": true,
        "headers": {
            "X-Template": "{{runtime.value}}"
        },
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096,
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    assert!(
        registry.create_tool(po).is_none(),
        "HTTP config validation should reject unsupported template placeholders before persistence/runtime"
    );
}

#[tokio::test]
async fn http_core_tool_does_not_follow_redirects_by_default() {
    let (base_url, server_handle) = start_json_server(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/metadata\r\nContent-Length: 0\r\n\r\n",
    );

    let po = http_tool_po(json!({
        "method": "GET",
        "url": format!("{}/redirect", base_url),
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096,
        "allowed_status_codes": [200],
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let error = tool
        .call(ctx, json!({ "query": "rust" }))
        .await
        .expect_err("redirect response should not be followed by default");

    let message = error.to_string();
    assert!(
        message.contains("unexpected http status code: 302"),
        "unexpected error message: {message}"
    );

    let request = server_handle
        .join()
        .expect("test HTTP server thread should finish");
    assert!(
        request.starts_with("GET /redirect "),
        "unexpected request line: {request:?}"
    );
}

#[tokio::test]
async fn http_core_tool_executes_get_request_against_registered_endpoint() {
    let (base_url, server_handle) = start_json_server(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 29\r\n\r\n{\"items\":[{\"name\":\"ai_orz\"}]}",
    );

    let po = http_tool_po(json!({
        "method": "GET",
        "url": format!("{}/search", base_url),
        "query": {
            "q": "{{args.query}}",
            "per_page": "{{args.limit}}"
        },
        "timeout_ms": 5_000,
        "response_max_bytes": 4_096,
        "allowed_status_codes": [200],
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let result = tool
        .call(ctx, json!({ "query": "rust", "limit": 3 }))
        .await
        .expect("HTTP core tool should execute GET request successfully");

    assert_eq!(result["status"], 200);
    assert_eq!(result["body"]["items"][0]["name"], "ai_orz");

    let request = server_handle
        .join()
        .expect("test HTTP server thread should finish");
    assert!(
        request.starts_with("GET /search?q=rust&per_page=3 ")
            || request.starts_with("GET /search?per_page=3&q=rust "),
        "unexpected request line: {request:?}"
    );
}

#[tokio::test]
async fn http_core_tool_renders_url_template_and_validates_required_args() {
    let (base_url, server_handle) = start_json_server(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
    );

    let po = http_tool_po(json!({
        "method": "GET",
        "url": format!("{}/repos/{{{{args.owner}}}}/{{{{args.repo}}}}", base_url),
        "timeout_ms": 5_000,
        "response_max_bytes": 4_096,
        "allowed_status_codes": [200],
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let missing_arg_error = tool
        .call(ctx.clone(), json!({ "owner": "openai" }))
        .await
        .expect_err("missing required schema args should be rejected before HTTP request");
    assert!(
        missing_arg_error
            .msg
            .contains("unknown tool argument: query"),
        "unexpected error message: {missing_arg_error}"
    );

    let result = tool
        .call(
            ctx,
            json!({ "query": "unused", "owner": "openai", "repo": "codex" }),
        )
        .await
        .expect("HTTP core tool should render URL template successfully");

    assert_eq!(result["status"], 200);
    assert_eq!(result["body"]["ok"], true);
    assert_eq!(result["content_length"], 11);
    assert!(result["headers"].is_object());

    let request = server_handle
        .join()
        .expect("test HTTP server thread should finish");
    assert!(
        request.starts_with("GET /repos/openai/codex "),
        "unexpected request line: {request:?}"
    );
}

#[tokio::test]
async fn http_core_tool_stops_reading_response_when_size_limit_is_exceeded() {
    let (base_url, server_handle) = start_json_server(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 12\r\n\r\nhello world!",
    );

    let po = http_tool_po(json!({
        "method": "GET",
        "url": format!("{}/large", base_url),
        "timeout_ms": 5_000,
        "response_max_bytes": 5,
        "allowed_status_codes": [200],
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let error = tool
        .call(ctx, json!({ "query": "rust" }))
        .await
        .expect_err("response larger than configured limit should be rejected");

    assert!(
        error.to_string().contains("http response too large"),
        "unexpected error message: {error}"
    );

    server_handle
        .join()
        .expect("test HTTP server thread should finish");
}

#[tokio::test]
async fn http_core_tool_redacts_rendered_url_from_request_errors() {
    let sensitive_value = "placeholder-value";
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://127.0.0.1:1/search/{{args.token}}",
        "query": {
            "access_token": "{{args.token}}"
        },
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096,
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let error = tool
        .call(ctx, json!({ "query": "rust", "token": sensitive_value }))
        .await
        .expect_err("connection failure should not expose rendered URL or secret args");

    let message = error.to_string();
    assert!(
        message.contains("http request failed"),
        "unexpected error message: {message}"
    );
    assert!(
        !message.contains(sensitive_value),
        "request error leaked rendered secret: {message}"
    );
    assert!(
        !message.contains("access_token"),
        "request error leaked query key: {message}"
    );
    assert!(
        !message.contains("127.0.0.1"),
        "request error leaked target URL: {message}"
    );
}

#[tokio::test]
async fn http_core_tool_redacts_template_derived_header_value_from_errors() {
    let sensitive_value = "placeholder-value";
    let po = http_tool_po(json!({
        "method": "GET",
        "url": "http://127.0.0.1:1/search",
        "allow_local_network": true,
        "headers": {
            "X-Test": "{{args.query}}"
        },
        "timeout_ms": 1_000,
        "response_max_bytes": 4_096,
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let error = tool
        .call(
            ctx,
            json!({ "query": format!("{sensitive_value}
") }),
        )
        .await
        .expect_err("invalid rendered header value should fail before network request");

    let message = error.to_string();
    assert!(
        message.contains("invalid http header value"),
        "unexpected error message: {message}"
    );
    assert!(
        !message.contains(sensitive_value),
        "header error leaked rendered secret: {message}"
    );
}

#[tokio::test]
async fn http_core_tool_redacts_body_read_errors() {
    let (base_url, server_handle) = start_json_server(
        "HTTP/1.1 200 OK
Content-Type: text/plain
Content-Length: 20

short",
    );

    let po = http_tool_po(json!({
        "method": "GET",
        "url": format!("{}/search/{{{{args.query}}}}", base_url),
        "timeout_ms": 5_000,
        "response_max_bytes": 4_096,
        "allow_local_network": true
    }));

    let registry = ToolRegistry::default();
    let tool = registry
        .create_tool(po)
        .expect("Http protocol ToolPo should create an executable HttpCoreTool");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    let sensitive_value = "placeholder-value";

    let error = tool
        .call(ctx, json!({ "query": sensitive_value }))
        .await
        .expect_err("truncated body should fail with a redacted read error");

    let message = error.to_string();
    assert!(
        message.contains("http response read failed"),
        "unexpected error message: {message}"
    );
    assert!(
        !message.contains(sensitive_value),
        "body read error leaked rendered URL value: {message}"
    );
    assert!(
        !message.contains("127.0.0.1"),
        "body read error leaked target URL: {message}"
    );

    server_handle
        .join()
        .expect("test HTTP server thread should finish");
}

fn start_json_server(response: &'static str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test HTTP server should bind");
    let address = listener
        .local_addr()
        .expect("test HTTP server should expose local address");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("test HTTP server should accept one request");
        let mut buffer = [0_u8; 1024];
        let size = stream
            .read(&mut buffer)
            .expect("test HTTP server should read request");
        stream
            .write_all(response.as_bytes())
            .expect("test HTTP server should write response");
        String::from_utf8_lossy(&buffer[..size]).to_string()
    });

    (format!("http://{}", address), handle)
}
