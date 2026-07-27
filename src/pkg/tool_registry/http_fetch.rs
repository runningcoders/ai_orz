//! Builtin HTTP Fetch Tool - fetch content from a dynamic HTTPS URL
//!
//! This is a generic builtin tool that allows the agent to fetch content from
//! any public HTTPS URL at runtime with strict security checks (SSRF protection,
//! size limits, no local network access by default).

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::BuiltinToolFactory;
use crate::pkg::tool_registry::tool_security::*;
use anyhow::anyhow;
use async_trait::async_trait;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
use reqwest::{Method, Url, redirect};
use serde_json::{Value, json};
use std::time::Duration;

/// Builtin HTTP fetch tool factory
#[derive(Debug, Clone, Default)]
pub struct HttpFetchToolFactory;

impl BuiltinToolFactory for HttpFetchToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "http_fetch".to_string(),
            name: "fetch_url".to_string(),
            description: "Fetch content from an HTTPS URL with GET method. Only public HTTPS URLs are allowed by default. Local network and HTTP URLs are rejected for security.".to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "HTTPS URL to fetch (HTTP is not allowed by default)."
                    }
                },
                "required": ["url"],
                "additionalProperties": false
            })),
            config: Value::Null,
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(HttpFetchCoreTool { po })
    }
}

/// Executable HTTP fetch core tool
#[derive(Debug, Clone)]
pub struct HttpFetchCoreTool {
    po: ToolPo,
}

#[async_trait]
impl CoreTool for HttpFetchCoreTool {
    async fn call(&self, _ctx: RequestContext, args: Value) -> Result<Value> {
        // Parse URL from arguments
        let url_str = match args.get("url") {
            Some(Value::String(s)) => s.clone(),
            _ => return Err(anyhow!("missing required argument 'url' must be a string").into()),
        };

        // Parse URL
        let url = Url::parse(&url_str)
            .map_err(|e| anyhow!("invalid URL: {}", e))
            .map_err(common::error::Error::from)?;

        // Security: only allow HTTPS by default
        if url.scheme() != "https" {
            return Err(anyhow!(
                "only HTTPS URLs are allowed for security reasons, got '{}'",
                url.scheme()
            )
            .into());
        }

        // Validate target with SSRF protection - default deny local network
        let pinned_addresses = validate_target_url(
            None, // allow_local_network: false (default deny)
            None, // no allowed_domains restrictions
            None, // no blocked_domains restrictions
            &url,
        )
        .await?;

        // Get host for DNS pinning
        let host = url
            .host_str()
            .ok_or_else(|| anyhow!("URL host is required"))
            .map_err(common::error::Error::from)?
            .to_string();

        // Build client with DNS pinning, no redirect, timeout
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
            .redirect(redirect::Policy::none())
            .no_proxy()
            .resolve_to_addrs(&host, &pinned_addresses)
            .build()
            .map_err(|e| {
                common::error::Error::new(common::error::ErrorCode::NetworkError, e.to_string())
            })?;

        // Send GET request
        let mut response = client
            .request(Method::GET, url)
            .send()
            .await
            .map_err(|e| anyhow!("HTTP request failed: {}", e))
            .map_err(common::error::Error::from)?;

        // Process response
        let status = response.status().as_u16();
        let headers = sanitize_response_headers(response.headers());
        let bytes = read_limited_response_body(&mut response, DEFAULT_RESPONSE_MAX_BYTES).await?;
        let content_length = bytes.len();

        // Parse body - try JSON first, fall back to string
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string()))
        };

        Ok(json!({
            "status": status,
            "headers": headers,
            "content_length": content_length,
            "body": body,
        }))
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Initialize storage before running tests
    async fn init() {
        crate::pkg::storage::test_support::init_for_test().await;
    }

    #[tokio::test]
    async fn test_fetch_public_https_ok() {
        init().await;
        let factory = HttpFetchToolFactory;
        let po = factory.create_po();
        let tool = HttpFetchCoreTool { po };

        // Fetch example.com - should succeed
        let result = tool
            .call(
                crate::pkg::request_context::RequestContext::new(None, None),
                json!({
                    "url": "https://example.com"
                }),
            )
            .await;

        assert!(
            result.is_ok(),
            "fetch example.com should succeed, got {:?}",
            result
        );
        let value = result.unwrap();
        assert!(value.get("status").is_some());
        assert!(value.get("status").unwrap().as_u64().unwrap() == 200);
    }

    #[tokio::test]
    async fn test_reject_http() {
        init().await;
        let factory = HttpFetchToolFactory;
        let po = factory.create_po();
        let tool = HttpFetchCoreTool { po };

        let result = tool
            .call(
                crate::pkg::request_context::RequestContext::new(None, None),
                json!({
                    "url": "http://example.com"
                }),
            )
            .await;

        assert!(result.is_err(), "should reject HTTP URLs");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("only HTTPS"));
    }

    #[tokio::test]
    async fn test_reject_localhost() {
        init().await;
        let factory = HttpFetchToolFactory;
        let po = factory.create_po();
        let tool = HttpFetchCoreTool { po };

        // Use https://localhost to test the localhost rejection
        let result = tool
            .call(
                crate::pkg::request_context::RequestContext::new(None, None),
                json!({
                    "url": "https://localhost:8080"
                }),
            )
            .await;

        assert!(result.is_err(), "should reject localhost");
        assert!(result.unwrap_err().to_string().contains("local network"));
    }

    #[tokio::test]
    async fn test_reject_private_ip() {
        init().await;
        let factory = HttpFetchToolFactory;
        let po = factory.create_po();
        let tool = HttpFetchCoreTool { po };

        // 192.168.1.1 is private
        let result = tool
            .call(
                crate::pkg::request_context::RequestContext::new(None, None),
                json!({
                    "url": "https://192.168.1.1"
                }),
            )
            .await;

        assert!(result.is_err(), "should reject private IP");
    }
}
