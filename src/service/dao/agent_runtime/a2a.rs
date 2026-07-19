//! A2A (Agent-to-Agent) Protocol Runtime DAO
//!
//! 通过 HTTP JSON-RPC 2.0 调用支持 A2A 协议的远程 Agent。
//! 遵循 Google A2A 协议规范（https://github.com/google/A2A）。
//!
//! 核心方法：agents/sendTask - 发送任务给远程 Agent 并等待结果

use async_trait::async_trait;
use common::error::{err, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use super::AgentRuntimeDao;

/// A2A 远程 Agent 执行配置
#[derive(Debug, Clone)]
pub struct A2aRuntimeConfig {
    /// 远程 Agent 的 A2A 端点 URL
    pub endpoint: String,
    /// 目标 Agent 名称（用于 agents/sendTask 的 agent_id 参数）
    pub agent_name: String,
    /// 认证 token（可选，通过 Authorization: Bearer <token> 传递）
    pub auth_token: Option<String>,
    /// 请求超时时间（秒）
    pub timeout_secs: u64,
}

/// A2A Runtime DAO
#[derive(Debug, Clone)]
pub struct A2aRuntimeDao {
    config: A2aRuntimeConfig,
    http: Client,
}

impl A2aRuntimeDao {
    pub fn new(config: A2aRuntimeConfig) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { config, http }
    }
}

// ==================== A2A Protocol Types ====================

/// A2A 消息内容部分
#[derive(Debug, Clone, Serialize, Deserialize)]
struct A2aMessagePart {
    #[serde(rename = "type")]
    part_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(flatten, skip_serializing_if = "Value::is_null")]
    extra: Value,
}

/// A2A 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
struct A2aMessage {
    role: String,
    parts: Vec<A2aMessagePart>,
}

/// JSON-RPC 请求
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    params: Value,
    id: u64,
}

/// JSON-RPC 响应
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
    #[allow(dead_code)]
    id: Option<Value>,
}

/// JSON-RPC 错误
#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

// ==================== Implementation ====================

#[async_trait]
impl AgentRuntimeDao for A2aRuntimeDao {
    async fn invoke(&self, _ctx: RequestContext, agent: &AgentPo, prompt: &str) -> Result<String> {
        execute_a2a(
            &self.http,
            &agent.id,
            &self.config.endpoint,
            &self.config.agent_name,
            &self.config.auth_token,
            prompt,
        ).await
    }
}

/// 执行 A2A agents/sendTask 调用
pub async fn execute_a2a(
    http: &Client,
    agent_id: &str,
    endpoint: &str,
    target_agent_name: &str,
    auth_token: &Option<String>,
    prompt: &str,
) -> Result<String> {
    let message = A2aMessage {
        role: "user".to_string(),
        parts: vec![A2aMessagePart {
            part_type: "text".to_string(),
            text: Some(prompt.to_string()),
            extra: json!({}),
        }],
    };

    let params = json!({
        "agent_id": target_agent_name,
        "message": message,
    });

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "agents/sendTask".to_string(),
        params,
        id: 1,
    };

    let mut req_builder = http
        .post(endpoint)
        .header("Content-Type", "application/json");

    if let Some(token) = auth_token {
        req_builder = req_builder.bearer_auth(token);
    }

    let response = req_builder
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            err!(
                Internal,
                "Agent {}: A2A HTTP request failed: {}",
                agent_id,
                e
            )
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(err!(
            Internal,
            "Agent {}: A2A HTTP error {}: {}",
            agent_id,
            status,
            body
        ));
    }

    let rpc_response: JsonRpcResponse = response.json().await.map_err(|e| {
        err!(
            Internal,
            "Agent {}: failed to parse A2A JSON-RPC response: {}",
            agent_id,
            e
        )
    })?;

    if let Some(rpc_error) = rpc_response.error {
        return Err(err!(
            Internal,
            "Agent {}: A2A JSON-RPC error {}: {}",
            agent_id,
            rpc_error.code,
            rpc_error.message
        ));
    }

    let result = rpc_response.result.unwrap_or_default();
    extract_text_from_result(&result)
        .ok_or_else(|| {
            err!(
                Internal,
                "Agent {}: A2A response has no text content: {}",
                agent_id,
                result
            )
        })
}

/// 从 A2A sendTask 结果中提取文本内容
///
/// A2A 协议的 sendTask 返回结果中包含 messages 数组，
/// 每个 message 有 parts 数组，每个 part 可能是 text 类型。
/// 我们提取所有 text part 的内容并拼接。
fn extract_text_from_result(result: &Value) -> Option<String> {
    let messages = result.get("messages")?.as_array()?;
    let mut texts = Vec::new();

    for msg in messages {
        let parts = msg.get("parts")?.as_array()?;
        for part in parts {
            if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    texts.push(text.to_string());
                }
            }
        }
    }

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_from_result_simple() {
        let result = json!({
            "messages": [
                {
                    "role": "assistant",
                    "parts": [
                        {"type": "text", "text": "Hello world"}
                    ]
                }
            ]
        });

        assert_eq!(
            extract_text_from_result(&result),
            Some("Hello world".to_string())
        );
    }

    #[test]
    fn test_extract_text_from_result_multiple_parts() {
        let result = json!({
            "messages": [
                {
                    "role": "assistant",
                    "parts": [
                        {"type": "text", "text": "Line 1"},
                        {"type": "text", "text": "Line 2"}
                    ]
                }
            ]
        });

        assert_eq!(
            extract_text_from_result(&result),
            Some("Line 1\nLine 2".to_string())
        );
    }

    #[test]
    fn test_extract_text_from_result_multiple_messages() {
        let result = json!({
            "messages": [
                {
                    "role": "assistant",
                    "parts": [{"type": "text", "text": "Part 1"}]
                },
                {
                    "role": "assistant",
                    "parts": [{"type": "text", "text": "Part 2"}]
                }
            ]
        });

        assert_eq!(
            extract_text_from_result(&result),
            Some("Part 1\nPart 2".to_string())
        );
    }

    #[test]
    fn test_extract_text_from_result_no_text_parts() {
        let result = json!({
            "messages": [
                {
                    "role": "assistant",
                    "parts": [
                        {"type": "image", "url": "http://example.com/img.png"}
                    ]
                }
            ]
        });

        assert_eq!(extract_text_from_result(&result), None);
    }

    #[test]
    fn test_extract_text_from_result_empty_messages() {
        let result = json!({
            "messages": []
        });

        assert_eq!(extract_text_from_result(&result), None);
    }

    #[test]
    fn test_extract_text_from_result_no_messages_field() {
        assert_eq!(extract_text_from_result(&json!({})), None);
    }

    #[test]
    fn test_a2a_message_serialization() {
        let msg = A2aMessage {
            role: "user".to_string(),
            parts: vec![A2aMessagePart {
                part_type: "text".to_string(),
                text: Some("hello".to_string()),
                extra: json!({}),
            }],
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["parts"][0]["type"], "text");
        assert_eq!(json["parts"][0]["text"], "hello");
    }

    #[test]
    fn test_json_rpc_request_serialization() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "agents/sendTask".to_string(),
            params: json!({"agent_id": "test"}),
            id: 42,
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "agents/sendTask");
        assert_eq!(json["params"]["agent_id"], "test");
        assert_eq!(json["id"], 42);
    }

    #[test]
    fn test_json_rpc_response_deserialization_result() {
        let json = json!({
            "jsonrpc": "2.0",
            "result": {"messages": []},
            "id": 1
        });

        let resp: JsonRpcResponse = serde_json::from_value(json).unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_json_rpc_response_deserialization_error() {
        let json = json!({
            "jsonrpc": "2.0",
            "error": {"code": -32601, "message": "Method not found"},
            "id": 1
        });

        let resp: JsonRpcResponse = serde_json::from_value(json).unwrap();
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }
}
