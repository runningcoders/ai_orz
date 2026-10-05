//! tests 单元测试（拆分自 a2a.rs）
//!
//! 文件瘦身：原 878 行 → 504 行，测试体 375 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

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

/// 进程内 stub A2A server：记录每次请求的 Key-Id 与声明头，
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
                let key_id = headers
                    .get("x-federation-key-id")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let decl = headers
                    .get("x-federation-caller")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                rec.lock().unwrap().push((key_id, decl));
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

/// 测试用客户端：同样走 http 基建，避免测试里出现无超时裸客户端
fn test_client() -> Client {
    crate::pkg::http::presets::outbound()
        .build()
        .expect("构建测试 HTTP 客户端失败")
}

fn federated_config(endpoint: String, deadline: u64, poll_ms: u64) -> FederatedCallConfig {
    let kp = crate::pkg::crypto::did::generate_keypair();
    FederatedCallConfig {
        endpoint,
        signing_key: kp.signing_key,
        peer_did: "did:key:z6MkPeerTest".to_string(),
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

    let http = test_client();
    let reply =
        execute_federated_agent_call(&http, "agt_x", &federated_config(endpoint, 10, 100), "hi")
            .await
            .unwrap();
    assert_eq!(reply, "pong");

    let rec = recorded.lock().unwrap();
    assert_eq!(rec.len(), 1, "同步型对端不应产生 tasks/get");
    let (key_id, decl) = &rec[0];
    assert!(
        key_id
            .as_deref()
            .is_some_and(|k| k.starts_with("did:key:z6Mk")),
        "签名头应携带本端 DID: {:?}",
        key_id
    );
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

    let http = test_client();
    let reply =
        execute_federated_agent_call(&http, "agt_x", &federated_config(endpoint, 10, 20), "hello")
            .await
            .unwrap();
    assert_eq!(reply, "echo-back");
    let rec = recorded.lock().unwrap();
    assert_eq!(rec.len(), 2, "send + 一次 get");
    // 轮询请求也必须携带声明头与签名头（对端日志全程带 org 维度，全程 fail-closed 验签）
    assert!(rec.iter().all(|(key_id, decl)| {
        key_id.is_some()
            && key_id.as_deref().unwrap_or("").starts_with("did:key:z6Mk")
            && decl.is_some()
    }));
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
    let http = test_client();
    let result =
        execute_federated_agent_call(&http, "agt_x", &federated_config(endpoint, 10, 100), "hi")
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
    let http = test_client();
    let result =
        execute_federated_agent_call(&http, "agt_x", &federated_config(endpoint, 1, 50), "hi")
            .await;
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("timed out"), "got: {}", err_msg);
}
