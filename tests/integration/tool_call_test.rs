//! 工具调用集成测试
//!
//! 覆盖：
//! - Part A: debug_call_tool 端点（Builtin/HTTP 工具调用 + SSRF 防护，CI-safe）
//! - Part B: Manual 异步消息链（Consumer 处理 ToolCallRequest → ToolCallResult，CI-safe）
//! - Part C: Auto awaken 工具执行（真实 LLM，#[ignore]）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use serde_json::json;
use sqlx::SqlitePool;
use std::io::{Read, Write};

/// 启动一个简单的 mock HTTP 服务器，返回固定响应。
/// 返回 (base_url, join_handle)，join_handle 返回接收到的请求文本。
fn start_mock_server(response: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind failed");
    let address = listener.local_addr().expect("local_addr failed");
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept failed");
        let mut buffer = [0u8; 1024];
        let size = stream.read(&mut buffer).expect("read failed");
        stream.write_all(response.as_bytes()).expect("write failed");
        String::from_utf8_lossy(&buffer[..size]).to_string()
    });
    (format!("http://{}", address), handle)
}

/// 创建 HTTP 工具，返回 tool_id
async fn create_http_tool(
    app: &TestApp,
    jwt: &str,
    name: &str,
    url: &str,
    allow_local_network: bool,
) -> String {
    let req = json!({
        "name": name,
        "description": "Test HTTP tool",
        "protocol": "Http",
        "config": {
            "method": "GET",
            "url": url,
            "allow_local_network": allow_local_network
        },
        "control_mode": "Manual",
        "enabled": true
    });
    let (status, body) = app.post_with_jwt("/api/v1/finance/tools", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing tool id")
        .to_string()
}

/// 调用 debug_call_tool，返回 (status, body)
///
/// 注意：`DebugCallToolRequest` 未 derive `Default`，宏走 path+body 混合分支，
/// JSON body 必须包含 `id` 字段（会被 path 值覆盖），否则反序列化失败返回 422。
async fn debug_call_tool(
    app: &TestApp,
    jwt: &str,
    tool_id: &str,
    args: &serde_json::Value,
) -> (axum::http::StatusCode, serde_json::Value) {
    let req = json!({ "id": tool_id, "args": args });
    app.post_with_jwt(
        &format!("/api/v1/finance/tools/{}/debug-call", tool_id),
        &req,
        jwt,
    )
    .await
}

// =================================================================
// Part A: debug_call_tool 端点测试（CI-safe，无 LLM）
// =================================================================

/// debug_call_tool 调用 HTTP 工具：创建指向 mock server 的 HTTP 工具 → debug-call → 验证结果
#[sqlx::test]
async fn test_debug_call_http_tool(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 启动 mock server
    let mock_response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 20\r\n\r\n{\"status\":\"success\"}";
    let (base_url, _handle) = start_mock_server(mock_response);

    // 2. 创建 HTTP 工具指向 mock server
    let tool_name = format!("HttpTool-{}", uuid::Uuid::now_v7());
    let tool_id =
        create_http_tool(&app, &jwt, &tool_name, &format!("{}/test", base_url), true).await;

    // 3. debug-call 调用
    let (status, body) = debug_call_tool(&app, &jwt, &tool_id, &json!({})).await;
    let data = crate::common::assert_api_ok(status, &body);

    // 4. 验证返回结果
    assert_eq!(data.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        data.get("status").and_then(|v| v.as_str()),
        Some("completed")
    );
    let result = data.get("result").expect("missing result");
    // HTTP 工具返回 { "status": 200, "headers": {...}, "body": {...} }
    assert_eq!(result.get("status").and_then(|v| v.as_u64()), Some(200));
    let body_content = result.get("body").expect("missing body");
    assert_eq!(
        body_content.get("status").and_then(|v| v.as_str()),
        Some("success")
    );
}

/// debug_call_tool SSRF 防护：创建指向 127.0.0.1 的 HTTP 工具（不设 allow_local_network）→ 调用 → 验证被拒绝
#[sqlx::test]
async fn test_debug_call_http_ssrf_blocked(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 创建指向 127.0.0.1 的 HTTP 工具，不设 allow_local_network
    // 注意：创建时可能就被 SSRF 校验拒绝（config_time 校验），所以需要检查创建是否成功
    let req = json!({
        "name": format!("SsrfTool-{}", uuid::Uuid::now_v7()),
        "description": "SSRF test tool",
        "protocol": "Http",
        "config": {
            "method": "GET",
            "url": "http://127.0.0.1:1/test",
            "allow_local_network": false
        },
        "control_mode": "Manual",
        "enabled": true
    });
    let (status, body) = app.post_with_jwt("/api/v1/finance/tools", &req, &jwt).await;

    // 创建时就应该被 SSRF 校验拒绝（config_time 校验 localhost/127.0.0.1）
    // 如果创建成功，debug-call 时会被运行时校验拒绝
    if status == axum::http::StatusCode::OK {
        // 创建成功了（不应该），尝试 debug-call 验证运行时拒绝
        let data = crate::common::assert_api_ok(status, &body);
        let tool_id = data
            .get("id")
            .and_then(|v| v.as_str())
            .expect("missing tool id");
        let (call_status, call_body) = debug_call_tool(&app, &jwt, tool_id, &json!({})).await;
        // 应该返回错误
        assert!(
            call_status != axum::http::StatusCode::OK
                || call_body.get("success").and_then(|v| v.as_bool()) == Some(false),
            "SSRF should block local network access"
        );
    } else {
        // 创建时被拒绝（预期行为）
        // body 应该包含错误信息
        let error_msg = body.to_string();
        assert!(
            error_msg.contains("local")
                || error_msg.contains("ssrf")
                || error_msg.contains("blocked")
                || error_msg.contains("network")
                || error_msg.contains("invalid"),
            "Error should mention local network/SSRF, got: {}",
            error_msg
        );
    }
}

/// debug_call_tool 调用不存在的工具 → 验证返回错误
#[sqlx::test]
async fn test_debug_call_tool_not_found(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let fake_tool_id = format!("nonexistent-{}", uuid::Uuid::now_v7());
    let (status, _body) = debug_call_tool(&app, &jwt, &fake_tool_id, &json!({})).await;

    // 应该返回 404 或错误
    assert!(
        status != axum::http::StatusCode::OK,
        "debug-call on non-existent tool should return error, got status: {}",
        status
    );
}
