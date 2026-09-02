//! Builtin HTTP Fetch Tool - fetch content from a dynamic HTTPS URL
//!
//! 核心抓取逻辑委托 `pkg::utils::fetch_remote_content`，工具层仅负责参数解析 + JSON 包装。

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::BuiltinToolFactory;
use crate::pkg::utils::fetch_remote_content::{FetchOptions, fetch_remote_content};
use anyhow::anyhow;
use async_trait::async_trait;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
use serde_json::{Value, json};

/// Builtin HTTP fetch tool factory
#[derive(Debug, Clone, Default)]
pub struct HttpFetchToolFactory;

impl BuiltinToolFactory for HttpFetchToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "http_fetch".to_string(),
            name: "Fetch Web Page".to_string(),
            description: "Fetch content from an HTTPS URL and return the response body plus status code. Only public HTTPS URLs are allowed by default — HTTP and local-network addresses are rejected for security. Use this when you just need raw HTML/text; use browser for JS-rendered pages that require a real browser.".to_string(),
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
            tags: serde_json::to_string(&vec!["http".to_string()]).unwrap_or_default(),
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

        // 工具场景配置：不跟随重定向、禁用代理、默认安全限制
        let options = FetchOptions {
            max_redirects: 0,
            no_proxy: true,
            ..Default::default()
        };

        let result = fetch_remote_content(&url_str, &options).await?;

        // Parse body - try JSON first, fall back to string
        let body = if result.bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&result.bytes).unwrap_or_else(|_| {
                Value::String(String::from_utf8_lossy(&result.bytes).to_string())
            })
        };

        Ok(json!({
            "status": result.status,
            "headers": result.headers,
            "content_length": result.bytes.len(),
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
                crate::pkg::request_context::RequestContext::new_system(),
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
                crate::pkg::request_context::RequestContext::new_system(),
                json!({
                    "url": "http://example.com"
                }),
            )
            .await;

        assert!(result.is_err(), "should reject HTTP URLs");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("only HTTPS") || err.to_string().contains("scheme"));
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
                crate::pkg::request_context::RequestContext::new_system(),
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
                crate::pkg::request_context::RequestContext::new_system(),
                json!({
                    "url": "https://192.168.1.1"
                }),
            )
            .await;

        assert!(result.is_err(), "should reject private IP");
    }
}
