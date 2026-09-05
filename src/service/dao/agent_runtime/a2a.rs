//! A2A (Agent-to-Agent) Protocol Runtime DAO
//!
//! 通过 HTTP JSON-RPC 2.0 调用支持 A2A 协议的远程 Agent。
//! 遵循 Google A2A 协议规范（https://github.com/google/A2A）。
//!
//! 核心方法：tasks/send - 发送任务给远程 Agent 并等待结果

use async_trait::async_trait;
use common::api::a2a::{
    A2aMessagePart, A2aTask, A2aTaskState, GetTaskParams, JsonRpcRequest, JsonRpcResponse,
    SendTaskParams,
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

/// 执行跨组织联邦 Agent 调用的配置（P4）
#[derive(Debug, Clone)]
pub struct FederatedCallConfig {
    /// 对端 A2A 端点（organization_links.endpoint）
    pub endpoint: String,
    /// 出站凭证（organization_links.access_token，对端所发，Bearer 传递）
    pub auth_token: String,
    /// `X-Federation-Caller` 声明头（已序列化的 JSON 明文；None = 连接级匿名）
    pub caller_declaration: Option<String>,
    /// send + poll 全程总预算（秒），超时返回错误
    pub deadline_secs: u64,
    /// tasks/get 轮询间隔（毫秒）
    pub poll_interval_ms: u64,
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
    extra_header: Option<(&str, &str)>,
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
    if let Some((name, value)) = extra_header {
        req_builder = req_builder.header(name, value);
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
        None,
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
        None,
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

/// 跨组织联邦 Agent 调用（P4）：tasks/send → 轮询 tasks/get 直到终态
///
/// 与 [`execute_a2a_send`] 的区别：
/// - 携带 `X-Federation-Caller` 声明头（R3 计量：对端日志带 org 维度）
/// - ai_orz 节点的 `tasks/send` 是异步提交（返回 working、无 assistant 文本），
///   需轮询 `tasks/get` 直到 Completed/Failed/Canceled；若 send 响应已带文本
///   且终态（同步型对端），首次检查即返回，不发多余的 get
pub async fn execute_federated_agent_call(
    http: &Client,
    agent_id: &str,
    config: &FederatedCallConfig,
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
            "Agent {}: failed to serialize federated params: {}",
            agent_id,
            e
        )
    })?;

    let context = format!("Federated agent {}", agent_id);
    let auth_token = Some(config.auth_token.clone());
    let extra_header = config
        .caller_declaration
        .as_deref()
        .map(|decl| (common::constants::http_header::FEDERATION_CALLER, decl));

    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_secs(config.deadline_secs);

    let mut task: A2aTask = {
        let result = call_a2a_jsonrpc(
            http,
            &config.endpoint,
            &auth_token,
            extra_header,
            "tasks/send",
            params_value,
            &context,
        )
        .await?;
        serde_json::from_value(result)
            .map_err(|e| err!(Internal, "{}: failed to parse A2aTask: {}", context, e))?
    };

    loop {
        match task.status.state {
            A2aTaskState::Completed => {
                return extract_text_from_task_result(
                    &serde_json::to_value(&task).unwrap_or_default(),
                )
                .ok_or_else(|| {
                    err!(
                        Internal,
                        "Agent {}: federated task completed but has no text content",
                        agent_id
                    )
                });
            }
            A2aTaskState::Failed | A2aTaskState::Canceled => {
                return Err(err!(
                    Internal,
                    "Agent {}: federated task ended with state {:?}",
                    agent_id,
                    task.status.state
                ));
            }
            A2aTaskState::InputRequired => {
                return Err(err!(
                    Internal,
                    "Agent {}: federated task requires input, interactive flow not supported",
                    agent_id
                ));
            }
            A2aTaskState::Submitted | A2aTaskState::Working => {}
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(err!(
                Internal,
                "Agent {}: federated task polling timed out after {}s",
                agent_id,
                config.deadline_secs
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(config.poll_interval_ms)).await;

        let get_params = GetTaskParams {
            id: task.id.clone(),
            history_length: None,
        };
        let result = call_a2a_jsonrpc(
            http,
            &config.endpoint,
            &auth_token,
            extra_header,
            "tasks/get",
            serde_json::to_value(&get_params).unwrap_or_default(),
            &context,
        )
        .await?;
        task = serde_json::from_value(result)
            .map_err(|e| err!(Internal, "{}: failed to parse A2aTask: {}", context, e))?;
    }
}

/// 从 A2A tasks/send 结果中提取文本内容
///
/// A2A 协议的 tasks/send 返回 Task 对象，包含 messages 数组，
/// 每个 message 有 parts 数组，每个 part 可能是 text 类型。
/// 我们提取所有 assistant role 的 text part 内容并拼接。
pub(crate) fn extract_text_from_task_result(result: &Value) -> Option<String> {
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
        assert_ne!(id1, id2);
    }

    // ==================== 联邦调用（P4）====================

    use std::sync::{Arc, Mutex};

    /// 进程内 stub A2A server：记录每次请求的 Bearer 与声明头，
    /// 按 `handler(request_json) -> Value` 的结果返回 JSON-RPC 响应。
    type RecordedHeaders = Vec<(Option<String>, Option<String>)>;

    async fn spawn_stub_a2a_server(
        handler: Arc<dyn Fn(Value) -> Value + Send + Sync>,
    ) -> (String, Arc<Mutex<RecordedHeaders>>) {
        use axum::routing::post;

        let recorded: Arc<Mutex<RecordedHeaders>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let app = axum::Router::new().route(
            "/a2a",
            post(
                move |headers: axum::http::HeaderMap, body: String| async move {
                    let req: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                    let bearer = headers
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let decl = headers
                        .get("x-federation-caller")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    rec.lock().unwrap().push((bearer, decl));
                    let rpc_id = req.get("id").cloned().unwrap_or(Value::Null);
                    let result = handler(req);
                    axum::Json(json!({
                        "jsonrpc": "2.0",
                        "id": rpc_id,
                        "result": result
                    }))
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{}/a2a", addr), recorded)
    }

    fn federated_config(endpoint: String, deadline: u64, poll_ms: u64) -> FederatedCallConfig {
        FederatedCallConfig {
            endpoint,
            auth_token: "fed-token-1".to_string(),
            caller_declaration: Some(r#"{"caller_org":"org-A","caller_user":"u-1"}"#.to_string()),
            deadline_secs: deadline,
            poll_interval_ms: poll_ms,
        }
    }

    #[tokio::test]
    async fn test_federated_call_sync_completion() {
        // 同步型对端：send 即返回 completed + agent 文本
        let (endpoint, recorded) = spawn_stub_a2a_server(Arc::new(|_req| {
            json!({
                "id": "t1",
                "status": {"state": "completed", "timestamp": "2026-01-01T00:00:00Z"},
                "messages": [
                    {"role": "user", "parts": [{"type": "text", "text": "hi"}]},
                    {"role": "agent", "parts": [{"type": "text", "text": "pong"}]}
                ]
            })
        }))
        .await;

        let http = Client::new();
        let reply = execute_federated_agent_call(
            &http,
            "agt_x",
            &federated_config(endpoint, 10, 100),
            "hi",
        )
        .await
        .unwrap();
        assert_eq!(reply, "pong");

        let rec = recorded.lock().unwrap();
        assert_eq!(rec.len(), 1, "同步型对端不应产生 tasks/get");
        let (bearer, decl) = &rec[0];
        assert_eq!(bearer.as_deref(), Some("Bearer fed-token-1"));
        assert_eq!(
            decl.as_deref(),
            Some(r#"{"caller_org":"org-A","caller_user":"u-1"}"#)
        );
    }

    #[tokio::test]
    async fn test_federated_call_send_then_poll() {
        // 异步型对端（ai_orz 节点行为）：send → working，get → completed
        let (endpoint, recorded) = spawn_stub_a2a_server(Arc::new(|req| {
            if req["method"] == "tasks/send" {
                json!({
                    "id": "t1",
                    "status": {"state": "working", "timestamp": "2026-01-01T00:00:00Z"},
                    "messages": []
                })
            } else {
                json!({
                    "id": "t1",
                    "status": {"state": "completed", "timestamp": "2026-01-01T00:00:01Z"},
                    "messages": [
                        {"role": "agent", "parts": [{"type": "text", "text": "echo-back"}]}
                    ]
                })
            }
        }))
        .await;

        let http = Client::new();
        let reply = execute_federated_agent_call(
            &http,
            "agt_x",
            &federated_config(endpoint, 10, 20),
            "hello",
        )
        .await
        .unwrap();
        assert_eq!(reply, "echo-back");
        let rec = recorded.lock().unwrap();
        assert_eq!(rec.len(), 2, "send + 一次 get");
        // 轮询请求也必须携带声明头（对端日志全程带 org 维度）
        assert!(rec.iter().all(|(_, decl)| decl.is_some()));
    }

    #[tokio::test]
    async fn test_federated_call_failed_state() {
        let (endpoint, _rec) = spawn_stub_a2a_server(Arc::new(|_req| {
            json!({
                "id": "t1",
                "status": {"state": "failed", "timestamp": "2026-01-01T00:00:00Z"},
                "messages": []
            })
        }))
        .await;
        let http = Client::new();
        let result = execute_federated_agent_call(
            &http,
            "agt_x",
            &federated_config(endpoint, 10, 100),
            "hi",
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_federated_call_timeout() {
        // 对端永远 working：应超时返回错误而不是无限挂起
        let (endpoint, _rec) = spawn_stub_a2a_server(Arc::new(|req| {
            if req["method"] == "tasks/send" {
                json!({
                    "id": "t1",
                    "status": {"state": "working", "timestamp": "2026-01-01T00:00:00Z"},
                    "messages": []
                })
            } else {
                json!({
                    "id": "t1",
                    "status": {"state": "working", "timestamp": "2026-01-01T00:00:01Z"},
                    "messages": []
                })
            }
        }))
        .await;
        let http = Client::new();
        let result =
            execute_federated_agent_call(&http, "agt_x", &federated_config(endpoint, 1, 50), "hi")
                .await;
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("timed out"), "got: {}", err_msg);
    }
}
