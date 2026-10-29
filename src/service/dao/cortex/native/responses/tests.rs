//! Responses 协议 wire 层单测
//!
//! 与 `http/tests.rs` 同规格：协议形状逐条配 twin test，重点锁死两类**静默**风险——
//! ① 有 function_call 却产出 Final（工具不执行但不报错）；② 终态缺失却返回半截结果。

use super::*;
use crate::pkg::request_context_test_support::new_test_ctx;
use futures_util::Stream;

/// 测试用 ctx（仅用于日志留痕路径；lazy pool 不会真实建连）
///
/// `SqlitePool::connect_lazy` 需要 tokio 上下文，故**用到 ctx 的用例必须是
/// `#[tokio::test]`**（与 `http/tests.rs` 同规格）。
fn ctx() -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").expect("lazy pool");
    new_test_ctx("test-user", pool)
}

fn obj(v: Value) -> ResponseObject {
    serde_json::from_value(v).expect("valid response object")
}

fn stream_of(chunks: Vec<String>) -> impl Stream<Item = reqwest::Result<Vec<u8>>> {
    futures_util::stream::iter(chunks.into_iter().map(|s| Ok(s.into_bytes())))
}

const IDLE: Duration = Duration::from_secs(1);

// ==================== 请求体 ====================

#[test]
fn request_body_has_store_false_and_flat_tools() {
    let provider = crate::models::model_provider::ModelProviderPo {
        id: "p".to_string(),
        name: "p".to_string(),
        provider_type: common::enums::ProviderType::OpenAI,
        model_name: "gpt".to_string(),
        capability: common::enums::ModelCapability::Agent,
        api_key: String::new(),
        base_url: None,
        description: None,
        config: "{}".to_string(),
        status: common::enums::ModelProviderStatus::Normal,
        created_by: "system".to_string(),
        modified_by: "system".to_string(),
        created_at: 0,
        updated_at: 0,
    };
    let tools = vec![ToolDescriptor {
        name: "list_tools".to_string(),
        description: "d".to_string(),
        parameters: serde_json::json!({"type": "object"}),
    }];
    let body = build_responses_body(
        &provider,
        &[ChatMessage::user("hi")],
        &tools,
        ModelAccessMode::Stream,
        None,
    );
    // 脱敏：默认服务端会留存 payload，必须显式关掉
    assert_eq!(body["store"], serde_json::json!(false));
    assert_eq!(body["stream"], serde_json::json!(true));
    // tools 扁平：没有 `function` 嵌套层
    let t = &body["tools"][0];
    assert_eq!(t["type"], "function");
    assert_eq!(t["name"], "list_tools");
    assert!(t.get("function").is_none(), "tools 不得保留 function 嵌套");
    // strict 恒 false：MCP 外部 schema 是不可信输入
    assert_eq!(t["strict"], serde_json::json!(false));
}

#[test]
fn request_body_non_stream_omits_stream_flag() {
    let provider = crate::models::model_provider::ModelProviderPo {
        id: "p".to_string(),
        name: "p".to_string(),
        provider_type: common::enums::ProviderType::OpenAI,
        model_name: "gpt".to_string(),
        capability: common::enums::ModelCapability::Agent,
        api_key: String::new(),
        base_url: None,
        description: None,
        config: "{}".to_string(),
        status: common::enums::ModelProviderStatus::Normal,
        created_by: "system".to_string(),
        modified_by: "system".to_string(),
        created_at: 0,
        updated_at: 0,
    };
    let body = build_responses_body(&provider, &[], &[], ModelAccessMode::NonStream, None);
    assert!(body.get("stream").is_none());
}

fn provider_with_config(config: &str) -> crate::models::model_provider::ModelProviderPo {
    crate::models::model_provider::ModelProviderPo {
        id: "p".to_string(),
        name: "p".to_string(),
        provider_type: common::enums::ProviderType::OpenAI,
        model_name: "gpt".to_string(),
        capability: common::enums::ModelCapability::Agent,
        api_key: String::new(),
        base_url: None,
        description: None,
        config: config.to_string(),
        status: common::enums::ModelProviderStatus::Normal,
        created_by: "system".to_string(),
        modified_by: "system".to_string(),
        created_at: 0,
        updated_at: 0,
    }
}

/// Provider 未配置 = 能力开关关闭：override 也不能激活该字段
#[test]
fn reasoning_omitted_when_provider_unconfigured() {
    let provider = provider_with_config("{}");
    let body = build_responses_body(
        &provider,
        &[],
        &[],
        ModelAccessMode::NonStream,
        Some(common::enums::ThinkingIntensity::MAX),
    );
    assert!(
        body.get("reasoning").is_none(),
        "provider 未配置时不得下发 reasoning: {body}"
    );
}

/// Provider 配置后：Responses 使用嵌套 `reasoning.effort` 结构，分数按三档转换
#[test]
fn reasoning_uses_provider_default_effort() {
    let provider = provider_with_config(r#"{"thinking_intensity":50}"#);
    let body = build_responses_body(&provider, &[], &[], ModelAccessMode::NonStream, None);
    assert_eq!(body["reasoning"]["effort"], json!("medium"));
}

/// Provider 配置后：override 优先，分数按三档转换
#[test]
fn reasoning_override_wins() {
    let provider = provider_with_config(r#"{"thinking_intensity":10}"#);
    let body = build_responses_body(
        &provider,
        &[],
        &[],
        ModelAccessMode::NonStream,
        Some(common::enums::ThinkingIntensity::MAX),
    );
    assert_eq!(body["reasoning"]["effort"], json!("high"));
}

// ==================== input 转换 ====================

#[test]
fn input_plain_user_uses_input_text_part() {
    let out = messages_to_input(&[ChatMessage::user("你好")]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["type"], "message");
    assert_eq!(out[0]["role"], "user");
    assert_eq!(out[0]["content"][0]["type"], "input_text");
    assert_eq!(out[0]["content"][0]["text"], "你好");
}

#[test]
fn input_system_kept_in_input_not_instructions() {
    // 系统消息可能多条（人设 / 技能 / 规则），合并进 instructions 会丢结构
    let out = messages_to_input(&[ChatMessage::system("s1"), ChatMessage::system("s2")]);
    assert_eq!(out.len(), 2, "多条 system 必须各自成为 item");
    assert_eq!(out[0]["role"], "system");
    assert_eq!(out[1]["role"], "system");
}

/// 一条 Assistant 展开成 1 个 message item + N 个 function_call item
#[test]
fn input_assistant_expands_to_message_plus_function_calls() {
    let msg = ChatMessage::Assistant {
        content: Some("我来查".to_string()),
        tool_calls: Some(vec![crate::models::cortex_types::ToolCallRequest {
            id: "call_1".to_string(),
            name: "list_tools".to_string(),
            arguments: serde_json::json!({"a": 1}),
        }]),
    };
    let out = messages_to_input(&[msg]);
    assert_eq!(out.len(), 2, "1 条 message + 1 个 function_call");
    assert_eq!(out[0]["type"], "message");
    assert_eq!(out[0]["role"], "assistant");
    assert_eq!(out[0]["content"][0]["type"], "output_text");
    assert_eq!(out[1]["type"], "function_call");
    assert_eq!(out[1]["call_id"], "call_1");
    assert_eq!(out[1]["name"], "list_tools");
    assert_eq!(out[1]["arguments"], "{\"a\":1}");
}

#[test]
fn input_assistant_with_empty_content_skips_message_item() {
    // 空 content 数组会被部分实现拒收
    let msg = ChatMessage::Assistant {
        content: None,
        tool_calls: Some(vec![crate::models::cortex_types::ToolCallRequest {
            id: "c".to_string(),
            name: "t".to_string(),
            arguments: serde_json::json!({}),
        }]),
    };
    let out = messages_to_input(&[msg]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["type"], "function_call");
}

#[test]
fn input_tool_result_uses_function_call_output() {
    let out = messages_to_input(&[ChatMessage::tool("call_9", "结果")]);
    assert_eq!(out[0]["type"], "function_call_output");
    assert_eq!(out[0]["call_id"], "call_9");
    assert_eq!(out[0]["output"], "结果");
}

/// 推理项回传：id + summary + encrypted_content 都要能原样往返
#[test]
fn input_reasoning_item_roundtrip() {
    let item = ReasoningItem {
        id: Some("rs_abc".to_string()),
        summary: vec!["第一步".to_string()],
        encrypted_content: Some("gAAA".to_string()),
    };
    let out = messages_to_input(&[ChatMessage::reasoning(item)]);
    assert_eq!(out[0]["type"], "reasoning");
    assert_eq!(out[0]["id"], "rs_abc");
    assert_eq!(out[0]["summary"][0]["type"], "summary_text");
    assert_eq!(out[0]["summary"][0]["text"], "第一步");
    assert_eq!(out[0]["encrypted_content"], "gAAA");
}

/// **核心守卫**：空 summary 必须输出 `"summary": []`，不得省略 key。
///
/// 部分网关（modelhub 实测）把字段缺失规范化成 `summary: null` 再校验，
/// 报 400 `Invalid type for 'input[N].summary': expected an array of
/// objects, but got null instead`。`store=false` + 模型未吐 summary 文本
/// 正是常态路径，省略 key 会让该 provider 每轮全挂。
#[test]
fn input_reasoning_item_empty_summary_is_empty_array_not_missing() {
    let item = ReasoningItem {
        id: Some("rs_abc".to_string()),
        summary: Vec::new(),
        encrypted_content: None,
    };
    let out = messages_to_input(&[ChatMessage::reasoning(item)]);
    assert_eq!(out[0]["type"], "reasoning");
    assert_eq!(
        out[0]["summary"],
        json!([]),
        "summary 缺失会被网关规范化成 null 而 400，必须显式给空数组"
    );
    assert!(
        out[0].get("encrypted_content").is_none(),
        "encrypted_content 为 None 时省略 key（该字段网关不强制）"
    );
}

// ==================== 终态解析 ====================

#[tokio::test]
async fn completed_with_text_yields_final() {
    let o = obj(json!({
        "status": "completed",
        "output": [{"type": "message", "content": [{"type": "output_text", "text": "答案"}]}],
        "usage": {"input_tokens": 5, "output_tokens": 7, "total_tokens": 12}
    }));
    match parse_response_object(&ctx(), o).expect("ok") {
        ThinkResult::Final { content, usage, .. } => {
            assert_eq!(content, "答案");
            assert_eq!(usage.input_tokens, 5);
            assert_eq!(usage.output_tokens, 7);
            assert_eq!(usage.total(), 12);
        }
        other => panic!("期望 Final，实际 {other:?}"),
    }
}

/// **核心守卫**：有 function_call 必须产出 ToolCall。
///
/// Responses 没有 finish_reason，判定只能靠扫描 output 数组；一旦漏判，表现是
/// 「模型要求调工具但被当成 Final 直接返回」——不报错、工具不执行，属静默退化。
#[tokio::test]
async fn completed_with_function_call_yields_tool_call() {
    let o = obj(json!({
        "status": "completed",
        "output": [{
            "type": "function_call",
            "call_id": "call_1",
            "name": "list_tools",
            "arguments": "{\"a\":1}"
        }],
        "usage": {"input_tokens": 10, "output_tokens": 2, "total_tokens": 12}
    }));
    match parse_response_object(&ctx(), o).expect("ok") {
        ThinkResult::ToolCall { tool_calls, .. } => {
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].id, "call_1");
            assert_eq!(tool_calls[0].name, "list_tools");
            assert_eq!(tool_calls[0].arguments, json!({"a": 1}));
        }
        other => panic!("期望 ToolCall（漏判即静默退化为 Final），实际 {other:?}"),
    }
}

#[tokio::test]
async fn mixed_message_and_function_call_keeps_both() {
    let o = obj(json!({
        "status": "completed",
        "output": [
            {"type": "message", "content": [{"type": "output_text", "text": "先查一下"}]},
            {"type": "function_call", "call_id": "c1", "name": "t", "arguments": "{}"}
        ]
    }));
    match parse_response_object(&ctx(), o).expect("ok") {
        ThinkResult::ToolCall {
            content,
            tool_calls,
            ..
        } => {
            assert_eq!(content.as_deref(), Some("先查一下"));
            assert_eq!(tool_calls.len(), 1);
        }
        other => panic!("期望 ToolCall，实际 {other:?}"),
    }
}

/// 未知 item 类型必须被忽略，而不是让整轮解析失败
#[tokio::test]
async fn unknown_output_item_types_are_ignored() {
    let o = obj(json!({
        "status": "completed",
        "output": [
            {"type": "web_search_call", "action": {"query": "x"}},
            {"type": "message", "content": [{"type": "output_text", "text": "ok"}]}
        ]
    }));
    match parse_response_object(&ctx(), o).expect("不得因未知 item 失败") {
        ThinkResult::Final { content, .. } => assert_eq!(content, "ok"),
        other => panic!("期望 Final，实际 {other:?}"),
    }
}

#[tokio::test]
async fn incomplete_fails_loud() {
    let o = obj(json!({
        "status": "incomplete",
        "incomplete_details": {"reason": "max_output_tokens"},
        "output": [{"type": "message", "content": [{"type": "output_text", "text": "半截"}]}]
    }));
    let e = parse_response_object(&ctx(), o).expect_err("截断必须显式失败");
    assert!(
        format!("{e}").contains("max_output_tokens"),
        "错误需带出原因：{e}"
    );
}

#[tokio::test]
async fn failed_fails_with_error_fields() {
    let o = obj(json!({
        "status": "failed",
        "error": {"code": "server_error", "message": "boom"}
    }));
    let e = parse_response_object(&ctx(), o).expect_err("failed 必须失败");
    let s = format!("{e}");
    assert!(
        s.contains("server_error") && s.contains("boom"),
        "错误需带出 code/message：{s}"
    );
}

#[tokio::test]
async fn missing_status_fails() {
    let o = obj(json!({"output": []}));
    assert!(
        parse_response_object(&ctx(), o).is_err(),
        "无 status 不得当成功"
    );
}

/// token 明细折算：Responses 的 input/output 命名 → 内部 prompt/completion 命名
#[tokio::test]
async fn usage_details_map_to_cached_and_reasoning_tokens() {
    let o = obj(json!({
        "status": "completed",
        "output": [{"type": "message", "content": [{"type": "output_text", "text": "x"}]}],
        "usage": {
            "input_tokens": 100,
            "output_tokens": 20,
            "total_tokens": 120,
            "input_tokens_details": {"cached_tokens": 60},
            "output_tokens_details": {"reasoning_tokens": 15}
        }
    }));
    let usage = parse_response_object(&ctx(), o)
        .expect("ok")
        .usage()
        .clone();
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 20);
    assert_eq!(usage.cached_tokens, 60, "缓存命中数必须落到统计链路");
    assert_eq!(usage.reasoning_tokens, 15, "思考开销必须落到统计链路");
}

#[tokio::test]
async fn reasoning_items_are_collected() {
    let o = obj(json!({
        "status": "completed",
        "output": [
            {"type": "reasoning", "id": "rs_1", "summary": [{"type":"summary_text","text":"想了一下"}], "encrypted_content": "gAAA"},
            {"type": "message", "content": [{"type": "output_text", "text": "ok"}]}
        ]
    }));
    let items = parse_response_object(&ctx(), o)
        .expect("ok")
        .reasoning_items()
        .to_vec();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id.as_deref(), Some("rs_1"));
    assert_eq!(items[0].summary, vec!["想了一下".to_string()]);
    assert_eq!(items[0].encrypted_content.as_deref(), Some("gAAA"));
}

// ==================== SSE 事件 ====================

#[test]
fn sse_delta_events_are_ignored() {
    for data in [
        r#"{"type":"response.created","response":{}}"#,
        r#"{"type":"response.output_text.delta","delta":"你"}"#,
        r#"{"type":"response.function_call_arguments.delta","delta":"{\"a\":"}"#,
        r#"{"type":"response.output_item.done","item":{}}"#,
    ] {
        let line = format!("data: {data}\n");
        assert!(
            matches!(
                parse_responses_sse_line(&line).expect("parse"),
                ResponsesSse::Ignore
            ),
            "delta/非终态事件应被忽略：{data}"
        );
    }
    assert!(matches!(
        parse_responses_sse_line("data: [DONE]\n").expect("parse"),
        ResponsesSse::Ignore
    ));
}

#[test]
fn sse_completed_is_terminal() {
    let line =
        r#"data: {"type":"response.completed","response":{"status":"completed","output":[]}}"#;
    match parse_responses_sse_line(line).expect("parse") {
        ResponsesSse::Terminal(o) => assert_eq!(o.status.as_deref(), Some("completed")),
        other => panic!(
            "期望 Terminal，实际 {}",
            matches!(other, ResponsesSse::Ignore)
        ),
    }
}

#[test]
fn sse_error_event_is_failed() {
    let line = r#"data: {"type":"error","code":"rate_limit","message":"slow down"}"#;
    match parse_responses_sse_line(line).expect("parse") {
        ResponsesSse::Failed(e) => {
            let s = format!("{e}");
            assert!(
                s.contains("rate_limit") && s.contains("slow down"),
                "错误需带出 code/message：{s}"
            );
        }
        _ => panic!("error 事件必须判为 Failed"),
    }
}

/// **核心守卫**：流内限流必须落到 `ModelRateLimited`，不得退化为 `Internal`
///
/// 退化的后果远比「文案不对」严重：`Internal` 不被 `is_model_error()` 认可，
/// AOP 消费者会按普通故障 nack 重投（见 `src/consumer/message.rs`）——在上游限流
/// 期间反复重试等于火上浇油，且**不会通知用户**；而 HTTP 429 路径是「不重试 +
/// 告知用户」。同一故障两种行为，由本组用例锁死为同口径。
#[test]
fn sse_error_event_rate_limit_maps_to_model_rate_limited() {
    let line = r#"data: {"type":"error","code":"rate_limit_exceeded","message":"slow down"}"#;
    match parse_responses_sse_line(line).expect("parse") {
        ResponsesSse::Failed(e) => {
            assert!(
                matches!(e.code_enum(), ErrorCode::ModelRateLimited),
                "流内限流必须归为 ModelRateLimited，实际 {:?}",
                e.code()
            );
            assert!(e.is_model_error(), "限流必须被识别为模型类错误");
            assert!(e.is_retryable(), "限流可重试");
        }
        _ => panic!("error 事件必须判为 Failed"),
    }
}

/// 流内 `response.failed` 事件同样是限流入口，与 `error` 事件同口径
#[test]
fn sse_response_failed_rate_limit_maps_to_model_rate_limited() {
    let line = r#"data: {"type":"response.failed","response":{"status":"failed","error":{"code":"rate_limit","message":"quota"}}}"#;
    match parse_responses_sse_line(line).expect("parse") {
        ResponsesSse::Failed(e) => {
            assert!(
                matches!(e.code_enum(), ErrorCode::ModelRateLimited),
                "response.failed 限流必须归为 ModelRateLimited，实际 {:?}",
                e.code()
            );
        }
        _ => panic!("response.failed 必须判为 Failed"),
    }
}

/// 非流式 / 终态 `status:"failed"` 是两条访问模式的汇合点，同样不得漏判
#[tokio::test]
async fn non_stream_failed_status_rate_limit_maps_to_model_rate_limited() {
    let o = obj(json!({
        "status": "failed",
        "error": {"code": "rate_limit", "message": "quota exceeded"}
    }));
    let e = parse_response_object(&ctx(), o).expect_err("failed 必须失败");
    assert!(
        matches!(e.code_enum(), ErrorCode::ModelRateLimited),
        "非流式 failed 限流必须归为 ModelRateLimited，实际 {:?}",
        e.code()
    );
    assert!(e.is_model_error(), "限流必须被识别为模型类错误");
}

/// 反向护栏：非限流错误**不得**被误判为限流（否则会错误地改变 AOP 的重投语义）
#[test]
fn sse_error_event_non_rate_limit_stays_internal() {
    let line = r#"data: {"type":"error","code":"server_error","message":"boom"}"#;
    match parse_responses_sse_line(line).expect("parse") {
        ResponsesSse::Failed(e) => {
            assert!(
                matches!(e.code_enum(), ErrorCode::Internal),
                "非限流错误不得被误判为限流，实际 {:?}",
                e.code()
            );
            assert!(!e.is_model_error());
        }
        _ => panic!("error 事件必须判为 Failed"),
    }
}

/// **核心守卫**：流在中途断掉且无终态事件时，不得返回半截结果
#[tokio::test]
async fn stream_without_terminal_event_fails() {
    let s = stream_of(vec![
        "event: response.output_text.delta\n".to_string(),
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"半截内容\"}\n".to_string(),
    ]);
    let e = consume_responses_stream(s, IDLE)
        .await
        .expect_err("连接中途断开必须显式失败");
    assert!(format!("{e}").contains("terminal"), "错误需说明缺终态：{e}");
}

#[tokio::test]
async fn stream_completed_returns_object() {
    let s = stream_of(vec![
        "data: {\"type\":\"response.created\",\"response\":{}}\n".to_string(),
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n".to_string(),
        concat!(
            r#"data: {"type":"response.completed","response":{"status":"completed","#,
            r#""output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}],"#,
            r#""usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}}"#,
            "\n"
        )
        .to_string(),
        "data: [DONE]\n".to_string(),
    ]);
    let o = consume_responses_stream(s, IDLE).await.expect("ok");
    assert_eq!(o.status.as_deref(), Some("completed"));
    match parse_response_object(&ctx(), o).expect("ok") {
        ThinkResult::Final { content, .. } => assert_eq!(content, "ok"),
        other => panic!("期望 Final，实际 {other:?}"),
    }
}

#[tokio::test]
async fn stream_failed_event_fails() {
    let s = stream_of(vec![
        r#"data: {"type":"response.failed","response":{"status":"failed","error":{"code":"x","message":"y"}}}"#
            .to_string()
            + "\n",
    ]);
    let e = consume_responses_stream(s, IDLE)
        .await
        .expect_err("failed 事件必须失败");
    assert!(format!("{e}").contains("y"), "{e}");
}
