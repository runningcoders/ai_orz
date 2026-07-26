//! A2A (Agent-to-Agent) Protocol Runtime DAO
//!
//! 通过 HTTP JSON-RPC 2.0 调用支持 A2A 协议的远程 Agent。
//! 遵循 Google A2A 协议规范（https://github.com/google/A2A）。
//!
//! 核心方法：tasks/send - 发送任务给远程 Agent 并等待结果

use async_trait::async_trait;
use common::api::a2a::{
    A2aMessagePart, A2aTask, GetTaskParams, JsonRpcRequest, JsonRpcResponse, SendTaskParams,
};
use common::error::{Result, err};
use reqwest::Client;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

use super::AgentRuntimeDao;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;

/// JSON-RPC 请求 ID 生成器（单调递增）
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> Value {
    let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    Value::Number(id.into())
}

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

    /// 调用 tasks/get 获取远程任务状态
    pub async fn fetch_task(&self, remote_task_id: &str) -> Result<A2aTask> {
        fetch_a2a_task(
            &self.http,
            &self.config.endpoint,
            &self.config.auth_token,
            remote_task_id,
        )
        .await
    }
}

// ==================== Implementation ====================

#[async_trait]
impl AgentRuntimeDao for A2aRuntimeDao {
    async fn invoke(&self, _ctx: RequestContext, agent: &AgentPo, prompt: &str) -> Result<String> {
        execute_a2a_send(
            &self.http,
            &agent.id,
            &self.config.endpoint,
            &self.config.auth_token,
            prompt,
        )
        .await
    }
}

/// 通用 A2A JSON-RPC 调用
async fn call_a2a_jsonrpc(
    http: &Client,
    endpoint: &str,
    auth_token: &Option<String>,
    method: &str,
    params: Value,
    context: &str,
) -> Result<Value> {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params,
        id: next_request_id(),
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
        .map_err(|e| err!(Internal, "{}: A2A HTTP request failed: {}", context, e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(err!(
            Internal,
            "{}: A2A HTTP error {}: {}",
            context,
            status,
            body
        ));
    }

    let rpc_response: JsonRpcResponse = response.json().await.map_err(|e| {
        err!(
            Internal,
            "{}: failed to parse A2A JSON-RPC response: {}",
            context,
            e
        )
    })?;

    if let Some(rpc_error) = rpc_response.error {
        return Err(err!(
            Internal,
            "{}: A2A JSON-RPC error {}: {}",
            context,
            rpc_error.code,
            rpc_error.message
        ));
    }

    Ok(rpc_response.result.unwrap_or_default())
}

/// 执行 A2A tasks/send 调用
pub async fn execute_a2a_send(
    http: &Client,
    agent_id: &str,
    endpoint: &str,
    auth_token: &Option<String>,
    prompt: &str,
) -> Result<String> {
    let task_id = uuid::Uuid::now_v7().to_string();

    let message = common::api::a2a::A2aMessage {
        role: "user".to_string(),
        parts: vec![A2aMessagePart::Text {
            text: prompt.to_string(),
        }],
        message_id: None,
        task_id: Some(task_id.clone()),
    };

    let params = SendTaskParams {
        id: task_id,
        message,
        session_id: None,
        metadata: None,
        notification_url: None,
    };

    let params_value = serde_json::to_value(&params).map_err(|e| {
        err!(
            Internal,
            "Agent {}: failed to serialize params: {}",
            agent_id,
            e
        )
    })?;

    let context = format!("Agent {}", agent_id);
    let result = call_a2a_jsonrpc(
        http,
        endpoint,
        auth_token,
        "tasks/send",
        params_value,
        &context,
    )
    .await?;

    extract_text_from_task_result(&result).ok_or_else(|| {
        err!(
            Internal,
            "Agent {}: A2A response has no text content: {}",
            agent_id,
            result
        )
    })
}

/// 执行 A2A tasks/get 调用，获取远程任务状态
pub async fn fetch_a2a_task(
    http: &Client,
    endpoint: &str,
    auth_token: &Option<String>,
    remote_task_id: &str,
) -> Result<A2aTask> {
    let params = GetTaskParams {
        id: remote_task_id.to_string(),
        history_length: None,
    };

    let params_value = serde_json::to_value(&params).map_err(|e| {
        err!(
            Internal,
            "Task {}: failed to serialize params: {}",
            remote_task_id,
            e
        )
    })?;

    let context = format!("Task {}", remote_task_id);
    let result = call_a2a_jsonrpc(
        http,
        endpoint,
        auth_token,
        "tasks/get",
        params_value,
        &context,
    )
    .await?;

    serde_json::from_value(result).map_err(|e| {
        err!(
            Internal,
            "Task {}: failed to parse A2aTask: {}",
            remote_task_id,
            e
        )
    })
}

/// 从 A2A tasks/send 结果中提取文本内容
///
/// A2A 协议的 tasks/send 返回 Task 对象，包含 messages 数组，
/// 每个 message 有 parts 数组，每个 part 可能是 text 类型。
/// 我们提取所有 assistant role 的 text part 内容并拼接。
fn extract_text_from_task_result(result: &Value) -> Option<String> {
    let task: common::api::a2a::A2aTask = serde_json::from_value(result.clone()).ok()?;

    let mut texts = Vec::new();

    for msg in &task.messages {
        if msg.role != "assistant" && msg.role != "agent" {
            continue;
        }
        for part in &msg.parts {
            if let A2aMessagePart::Text { text } = part {
                texts.push(text.clone());
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
    use serde_json::json;

    #[test]
    fn test_extract_text_from_task_result_simple() {
        let result = json!({
            "id": "task-1",
            "status": {"state": "completed", "timestamp": "2024-01-01T00:00:00Z"},
            "messages": [
                {
                    "role": "user",
                    "parts": [{"type": "text", "text": "Hello"}]
                },
                {
                    "role": "agent",
                    "parts": [{"type": "text", "text": "Hello world"}]
                }
            ]
        });

        assert_eq!(
            extract_text_from_task_result(&result),
            Some("Hello world".to_string())
        );
    }

    #[test]
    fn test_extract_text_from_task_result_multiple_parts() {
        let result = json!({
            "id": "task-1",
            "status": {"state": "completed", "timestamp": "2024-01-01T00:00:00Z"},
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
            extract_text_from_task_result(&result),
            Some("Line 1\nLine 2".to_string())
        );
    }

    #[test]
    fn test_extract_text_from_task_result_multiple_messages() {
        let result = json!({
            "id": "task-1",
            "status": {"state": "completed", "timestamp": "2024-01-01T00:00:00Z"},
            "messages": [
                {
                    "role": "assistant",
                    "parts": [{"type": "text", "text": "Part 1"}]
                },
                {
                    "role": "agent",
                    "parts": [{"type": "text", "text": "Part 2"}]
                }
            ]
        });

        assert_eq!(
            extract_text_from_task_result(&result),
            Some("Part 1\nPart 2".to_string())
        );
    }

    #[test]
    fn test_extract_text_from_task_result_no_text_parts() {
        let result = json!({
            "id": "task-1",
            "status": {"state": "completed", "timestamp": "2024-01-01T00:00:00Z"},
            "messages": [
                {
                    "role": "assistant",
                    "parts": [
                        {"type": "file", "file": {"name": "test.txt"}}
                    ]
                }
            ]
        });

        assert_eq!(extract_text_from_task_result(&result), None);
    }

    #[test]
    fn test_extract_text_from_task_result_empty_messages() {
        let result = json!({
            "id": "task-1",
            "status": {"state": "working", "timestamp": "2024-01-01T00:00:00Z"},
            "messages": []
        });

        assert_eq!(extract_text_from_task_result(&result), None);
    }

    #[test]
    fn test_extract_text_from_task_result_no_messages_field() {
        assert_eq!(extract_text_from_task_result(&json!({})), None);
    }

    #[test]
    fn test_json_rpc_request_serialization() {
        let params = SendTaskParams {
            id: "task-1".to_string(),
            message: common::api::a2a::A2aMessage {
                role: "user".to_string(),
                parts: vec![A2aMessagePart::Text {
                    text: "hello".to_string(),
                }],
                message_id: None,
                task_id: Some("task-1".to_string()),
            },
            session_id: None,
            metadata: None,
            notification_url: None,
        };

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "tasks/send".to_string(),
            params: serde_json::to_value(&params).unwrap(),
            id: Value::Number(42.into()),
        };

        let json_val = serde_json::to_value(&request).unwrap();
        assert_eq!(json_val["jsonrpc"], "2.0");
        assert_eq!(json_val["method"], "tasks/send");
        assert_eq!(json_val["params"]["id"], "task-1");
        assert_eq!(json_val["params"]["message"]["role"], "user");
        assert_eq!(json_val["id"], 42);
    }

    #[test]
    fn test_json_rpc_response_deserialization_result() {
        let json_val = json!({
            "jsonrpc": "2.0",
            "result": {
                "id": "task-1",
                "status": {"state": "working", "timestamp": "2024-01-01T00:00:00Z"},
                "messages": []
            },
            "id": 1
        });

        let resp: JsonRpcResponse = serde_json::from_value(json_val).unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_json_rpc_response_deserialization_error() {
        let json_val = json!({
            "jsonrpc": "2.0",
            "error": {"code": -32601, "message": "Method not found"},
            "id": 1
        });

        let resp: JsonRpcResponse = serde_json::from_value(json_val).unwrap();
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }

    #[test]
    fn test_next_request_id() {
        let id1 = next_request_id();
        let id2 = next_request_id();
        assert!(id1.is_number());
        assert!(id2.is_number());
        assert_ne!(id1, id2);
    }
}
