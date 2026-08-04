//! 工具调用集成测试
//!
//! 覆盖：
//! - Part A: debug_call_tool 端点（Builtin/HTTP 工具调用 + SSRF 防护，CI-safe）
//! - Part B: Manual 异步消息链（Consumer 处理 ToolCallRequest → ToolCallResult，CI-safe）
//! - Part C: Auto awaken 工具执行（真实 LLM，#[ignore]）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ai_orz::consumer::message::MessageConsumer;
use ai_orz::pkg::RequestContext;
use ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager;
use ai_orz::pkg::aop::Consumer;
use ai_orz::pkg::tool_tracing::entry::ToolCallStatus;
use ai_orz::pkg::tool_tracing::logger::ToolCallQuery;
use ai_orz::service::domain::message::{self, SendToolCallRequestCommand};
use ai_orz::service::domain::runtime;
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

/// 绑定工具到 Agent
async fn bind_tool_to_agent(app: &TestApp, jwt: &str, agent_id: &str, tool_id: &str) {
    let (status, _body) = app
        .post_with_jwt(
            &format!("/api/v1/hr/agents/{}/tools/{}/bind", agent_id, tool_id),
            &serde_json::json!({}),
            jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "tool bind should succeed"
    );
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

// =================================================================
// Part B: Manual 异步消息链（Consumer 处理 ToolCallRequest → ToolCallResult）
// =================================================================

/// Consumer 处理 ToolCallRequest：创建 ToolCallRequest 消息 → Consumer 执行工具 → 验证 ToolCallResult 消息生成
#[sqlx::test]
async fn test_consumer_tool_call_request_chain(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 启动 mock server
    let mock_response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 20\r\n\r\n{\"status\":\"success\"}";
    let (base_url, _handle) = start_mock_server(mock_response);

    // 2. 创建 HTTP 工具
    let tool_name = format!("ManualTool-{}", uuid::Uuid::now_v7());
    let tool_id =
        create_http_tool(&app, &jwt, &tool_name, &format!("{}/exec", base_url), true).await;

    // 3. 创建 Agent
    let agent_name = format!("ToolCallAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &agent_name)
            .await;

    // 4. 绑定工具到 Agent
    bind_tool_to_agent(&app, &jwt, &agent_id, &tool_id).await;

    // 5. 创建 ToolCallRequest 消息
    // 关键：ctx 需要设置 organization_id，否则消息写入时 org 为 None，
    // 后续 JWT 查询（带 org scope）无法匹配到消息
    let ctx = ctx
        .to_builder()
        .organization_id(bs.organization_id.clone())
        .build();

    let request_id = uuid::Uuid::now_v7().to_string();
    let cmd = SendToolCallRequestCommand {
        request_id: &request_id,
        tool_id: &tool_id,
        tool_name: &tool_name,
        from_agent_id: &agent_id,
        to_executor_id: "system",
        project_id: None,
        task_id: None,
        reply_to_id: None,
        args: serde_json::json!({}),
    };

    let message = message::domain()
        .delivery()
        .send_tool_call_request(ctx, cmd)
        .await
        .expect("send_tool_call_request should succeed");
    let message_id = message.po.id.clone();

    // 6. 调用 Consumer 处理消息事件
    let consumer = MessageConsumer::new();
    let event = serde_json::json!({
        "message_id": message_id,
        "project_id": null,
        "task_id": null,
        "from_id": agent_id,
        "from_role": 1,
        "to_id": "system",
        "to_role": 2,
        "message_type": 5,
        "content": "",
        "created_at": 0
    });

    let result = consumer.on_event(event).await;

    // Consumer 应该成功处理
    assert!(
        result.is_ok(),
        "Consumer should succeed processing ToolCallRequest, got: {:?}",
        result.err()
    );

    // 7. 验证 ToolCallResult 消息生成
    // ToolCallResult 消息 from_role=System(2), to_role=Agent(1)
    // 等待消息写入
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let (status, body) = app
        .get_with_jwt(
            &format!("/api/v1/finance/messages?from_id=system&to_id={}", agent_id),
            &jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    let messages = data
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("missing messages array");

    // 应该至少有一条 ToolCallResult 消息（message_type=6）
    let tool_call_result = messages
        .iter()
        .find(|msg| msg.get("message_type").and_then(|v| v.as_i64()) == Some(6));
    assert!(
        tool_call_result.is_some(),
        "Should find a ToolCallResult message (type=6)"
    );

    // 验证 ToolCallResult 内容
    if let Some(result_msg) = tool_call_result {
        let content = result_msg
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        // content 是 ToolCallMessage JSON
        let tool_call_msg: serde_json::Value =
            serde_json::from_str(content).expect("ToolCallResult content should be valid JSON");
        assert_eq!(
            tool_call_msg.get("is_success").and_then(|v| v.as_bool()),
            Some(true),
            "Tool call should be successful"
        );
    }
}

/// 工具调用 trace 记录验证：执行工具后查询 trace
///
/// 注意：trace 查询 API（HTTP）要求 request context 有 scope（agent_id/project_id/task_id），
/// 而 debug_call_tool 不设置 agent_id scope，无法通过 HTTP 查询。
/// 因此本测试通过 domain 层直接调用 `call_manual_tool_for_agent`（带 agent_id scope），
/// 再通过 domain 层 `query_tool_call_entries` 查询 trace，验证记录被正确写入。
#[sqlx::test]
async fn test_tool_call_trace_recorded(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 启动 mock server
    let mock_response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 20\r\n\r\n{\"status\":\"success\"}";
    let (base_url, _handle) = start_mock_server(mock_response);

    // 2. 创建 HTTP 工具
    let tool_name = format!("TraceTool-{}", uuid::Uuid::now_v7());
    let tool_id =
        create_http_tool(&app, &jwt, &tool_name, &format!("{}/trace", base_url), true).await;

    // 3. 创建 Agent 并绑定工具（call_manual_tool_for_agent 需要工具已绑定到 Agent）
    let agent_name = format!("TraceAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &agent_name)
            .await;
    bind_tool_to_agent(&app, &jwt, &agent_id, &tool_id).await;

    // 4. 通过 domain 层调用工具（设置 organization_id + agent_id scope）
    let call_ctx = RequestContext::builder()
        .organization_id(bs.organization_id.clone())
        .build();

    let result = runtime::domain()
        .tool_execution()
        .call_manual_tool_for_agent(call_ctx, agent_id.clone(), tool_id.clone(), json!({}))
        .await
        .expect("call_manual_tool_for_agent failed");
    let tool_call_id = result.trace_ref.call_id.clone();

    // 5. 通过 domain 层查询 trace（需要 agent_id scope 通过 with_context_scope 校验）
    let query_ctx = RequestContext::builder().agent_id(agent_id.clone()).build();

    let entries = runtime::domain()
        .tool_execution()
        .query_tool_call_entries(
            query_ctx,
            ToolCallQuery {
                tool_id: Some(tool_id.clone()),
                agent_id: Some(agent_id.clone()),
                limit: Some(10),
                ..Default::default()
            },
        )
        .await
        .expect("trace query failed");

    // 应该至少有一条 trace 记录
    assert!(
        !entries.is_empty(),
        "Should find at least 1 trace entry for tool_id"
    );

    // 找到我们刚才的调用
    let our_trace = entries.iter().find(|entry| entry.call_id == tool_call_id);
    assert!(
        our_trace.is_some(),
        "Should find trace entry with call_id: {}",
        tool_call_id
    );

    if let Some(entry) = our_trace {
        assert_eq!(
            entry.tool_id, tool_id,
            "Trace entry should have correct tool_id"
        );
        assert_eq!(
            entry.status,
            ToolCallStatus::Completed,
            "Trace entry should have status Completed"
        );
    }
}

// =================================================================
// Part C: Auto awaken 工具执行（真实 LLM，#[ignore]）
// =================================================================

/// Parse provider type string to serde variant name.
///
/// env 变量值（如 "doubao"）需转换为 serde 变体名（如 "Doubao"）才能被
/// `ProviderType` 的 `Deserialize` 正确解析（无 `rename_all`，默认按变体名匹配）。
fn parse_provider_type(s: &str) -> &'static str {
    match s.to_lowercase().as_str() {
        "openai" | "0" => "OpenAI",
        "deepseek" | "1" => "DeepSeek",
        "qwen" | "2" => "Qwen",
        "doubao" | "3" => "Doubao",
        "ollama" | "4" => "Ollama",
        "custom" | "5" => "Custom",
        "fastembed" | "6" => "FastEmbed",
        "doubao_vision" | "doubaoVision" | "7" => "DoubaoVision",
        _ => "OpenAI",
    }
}

/// 真实模型配置（从 .env 读取）
struct RealLlmConfig {
    llm_provider_type: &'static str,
    llm_model_name: String,
    llm_api_key: String,
    llm_base_url: Option<String>,
}

impl RealLlmConfig {
    fn from_env() -> Option<Self> {
        let _ = dotenvy::dotenv();
        let llm_api_key = std::env::var("TEST_LLM_API_KEY").ok()?;
        let llm_model_name = std::env::var("TEST_LLM_MODEL_NAME").ok()?;
        let llm_provider_type = std::env::var("TEST_LLM_PROVIDER_TYPE")
            .ok()
            .as_deref()
            .map(parse_provider_type)
            .unwrap_or("Doubao");
        let llm_base_url = std::env::var("TEST_LLM_BASE_URL").ok();
        Some(Self {
            llm_provider_type,
            llm_model_name,
            llm_api_key,
            llm_base_url,
        })
    }
}

/// 创建真实 LLM Provider，返回 provider_id
async fn create_real_llm_provider(app: &TestApp, jwt: &str, cfg: &RealLlmConfig) -> String {
    let req = json!({
        "name": format!("RealLLM-{}", uuid::Uuid::now_v7()),
        "provider_type": cfg.llm_provider_type,
        "capability": "Agent",
        "model_name": cfg.llm_model_name,
        "api_key": cfg.llm_api_key,
        "base_url": cfg.llm_base_url,
        "description": "Real LLM for auto tool call test"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/model-providers", &req, jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing provider id")
        .to_string()
}

/// 启动多请求 mock HTTP 服务器（循环处理，支持 LLM 多次调用工具）
fn start_mock_server_multi() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind failed");
    let address = listener.local_addr().expect("local_addr failed");
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 26\r\n\r\n{\"result\":\"data from api\"}";
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{}", address)
}

/// 真实 LLM + Auto 工具端到端测试：
/// 创建 Agent + Auto HTTP 工具 → 发消息 → Consumer 触发 awaken → LLM 调用工具 → 验证 trace
///
/// 验证：
/// - Agent 收到消息后触发 awaken
/// - 真实 LLM 在 awaken 过程中调用 Auto HTTP 工具
/// - 工具调用 trace 被记录
/// - Agent 回到 Idle 状态
#[sqlx::test]
#[ignore = "requires real LLM API key in .env (TEST_LLM_API_KEY)"]
async fn test_real_llm_auto_tool_call(pool: SqlitePool) {
    let Some(cfg) = RealLlmConfig::from_env() else {
        eprintln!("SKIP: TEST_LLM_API_KEY not set, skipping auto tool call test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 启动 mock server
    let mock_url = start_mock_server_multi();

    // 2. 创建真实 LLM Provider
    let real_provider_id = create_real_llm_provider(&app, &jwt, &cfg).await;

    // 3. 创建 Auto HTTP 工具（control_mode = Auto，会被注入 Rig 供 LLM 调用）
    let tool_req = json!({
        "name": "fetch_data",
        "description": "Fetch data from an API endpoint. Returns JSON with result field.",
        "protocol": "Http",
        "config": {
            "method": "GET",
            "url": format!("{}/data", mock_url),
            "allow_local_network": true
        },
        "parameters_schema": {
            "type": "object",
            "properties": {},
            "required": []
        },
        "control_mode": "Auto",
        "enabled": true,
        "tags": ["test"]
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/tools", &tool_req, &jwt)
        .await;
    let tool_data = crate::common::assert_api_ok(status, &body);
    let tool_id = tool_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing tool id")
        .to_string();

    // 4. 创建 Agent（使用真实 LLM Provider）
    let agent_name = format!("AutoToolAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &real_provider_id, &agent_name)
            .await;

    // 5. 绑定工具到 Agent
    bind_tool_to_agent(&app, &jwt, &agent_id, &tool_id).await;

    // 6. 发送消息，引导 LLM 使用工具
    let send_req = json!({
        "to_agent_id": agent_id,
        "content": "请使用 fetch_data 工具获取数据，然后告诉我结果"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string();

    // 7. 调用 Consumer 触发 awaken（真实 LLM 调用）
    let consumer = MessageConsumer::new();
    let event = json!({
        "message_id": message_id,
        "project_id": null,
        "task_id": null,
        "from_id": bs.user_id,
        "from_role": 0,
        "to_id": agent_id,
        "to_role": 1,
        "message_type": 0,
        "content": "",
        "created_at": 0
    });

    let result = consumer.on_event(event).await;

    // awaken 应该成功
    assert!(
        result.is_ok(),
        "Consumer should succeed with real LLM, got: {:?}",
        result.err()
    );

    // 8. 验证 Agent 回到 Idle
    let runtime_state = AgentRuntimeStateManager::global();
    let state = runtime_state.get_state(&agent_id);
    assert_eq!(
        state,
        ::common::enums::AgentRuntimeState::Idle,
        "Agent should be Idle after awaken completion"
    );

    // 9. 验证工具被调用（查询 trace）
    // 等待异步 trace 写入完成
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let query_ctx = RequestContext::builder().agent_id(agent_id.clone()).build();
    let entries = runtime::domain()
        .tool_execution()
        .query_tool_call_entries(
            query_ctx,
            ToolCallQuery {
                tool_id: Some(tool_id.clone()),
                agent_id: Some(agent_id.clone()),
                limit: Some(10),
                ..Default::default()
            },
        )
        .await
        .expect("trace query failed");

    assert!(
        !entries.is_empty(),
        "Should find at least 1 trace entry for tool_id (LLM should have called the tool)"
    );

    eprintln!(
        "Real LLM auto tool call test passed! {} trace entries found.",
        entries.len()
    );

    // Cleanup
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", real_provider_id),
            &jwt,
        )
        .await;
}
