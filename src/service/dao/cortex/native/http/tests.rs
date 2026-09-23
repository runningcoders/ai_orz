use super::*;
use crate::pkg::http::HttpClientOptions;
use crate::pkg::request_context_test_support::new_test_ctx;
use common::error::ErrorCode;
use futures_util::Stream;
use reqwest::StatusCode;

fn sc(u: u16) -> StatusCode {
    StatusCode::from_u16(u).unwrap()
}

/// 测试用 ctx（仅用于日志留痕路径；lazy pool 不会真实建连）
///
/// `SqlitePool::connect_lazy` 需要 tokio 上下文，故用到它的用例必须是 `#[tokio::test]`。
fn test_ctx() -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").expect("lazy pool");
    new_test_ctx("test-user", pool)
}

/// 将若干原始字节段拼成 SSE 字节流（可含在 chunk 边界被截断的行 / JSON）
fn stream_of(chunks: Vec<String>) -> impl Stream<Item = reqwest::Result<Vec<u8>>> {
    futures_util::stream::iter(chunks.into_iter().map(|s| Ok(s.into_bytes())))
}

fn chunk_of(json: &str) -> StreamChunk {
    serde_json::from_str(json).expect("valid stream chunk json")
}

const IDLE: Duration = Duration::from_secs(1);

#[test]
fn classifies_rate_limit_as_retryable() {
    let e = model_call_error("chat", sc(429), "slow down");
    assert!(e.is_model_error());
    assert!(e.is_retryable());
    assert!(matches!(e.code, ErrorCode::ModelRateLimited));
}

#[test]
fn classifies_5xx_as_server_retryable() {
    for s in [500u16, 502, 503, 599] {
        let e = model_call_error("chat", sc(s), "boom");
        assert!(e.is_retryable(), "status {s} should be retryable");
        assert!(
            matches!(e.code, ErrorCode::ModelServerError),
            "status {s} -> server error"
        );
    }
}

#[test]
fn classifies_auth_as_non_retryable() {
    for s in [401u16, 403] {
        let e = model_call_error("chat", sc(s), "no auth");
        assert!(!e.is_retryable(), "status {s} should NOT be retryable");
        assert!(matches!(e.code, ErrorCode::ModelAuth));
    }
}

#[test]
fn classifies_4xx_as_bad_request_unless_content_filtered() {
    let bad = model_call_error("chat", sc(400), "invalid param");
    assert!(matches!(bad.code, ErrorCode::ModelBadRequest));
    assert!(!bad.is_retryable());

    let filtered = model_call_error("chat", sc(400), "error: content_filter triggered");
    assert!(matches!(filtered.code, ErrorCode::ModelContentFiltered));
    assert!(!filtered.is_retryable());

    let moderation = model_call_error("chat", sc(422), "{\"type\":\"content_filter\"}");
    assert!(matches!(moderation.code, ErrorCode::ModelContentFiltered));
}

#[tokio::test]
async fn classifies_transport_error_with_root_cause() {
    let opts = HttpClientOptions::new().no_proxy().timeout_ms(2_000);
    let client = opts.build().expect("client build");
    let e = client
        .post("http://127.0.0.1:1/chat/completions")
        .send()
        .await
        .expect_err("connect to refused port must fail");
    assert!(e.is_connect(), "expected connect error, got: {e}");

    let err = transport_error("chat completions", e);
    assert!(matches!(err.code, ErrorCode::Internal));
    assert!(err.msg.contains("(connect)"), "msg: {}", err.msg);
    assert!(
        err.msg.starts_with("chat completions request failed"),
        "msg: {}",
        err.msg
    );
    assert!(err.msg.contains("root cause"), "msg: {}", err.msg);
    assert!(!err.msg.contains("<no cause>"), "msg: {}", err.msg);
}

#[tokio::test]
async fn aggregates_content_and_usage_from_fragmented_bytes() {
    let chunks = vec![
        // 首个 chunk 的 data 行在 JSON 字符串中间截断且不带换行
        "data: {\"choices\":[{\"delta\":{\"content\":\"He".to_string(),
        // 换行补齐首行，再跟一条完整 data 行
        "llo\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n".to_string(),
        // usage 随最后一个 chunk（choices 为空）下发
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n".to_string(),
        "data: [DONE]\n".to_string(),
    ];
    let acc = consume_think_stream(stream_of(chunks), IDLE)
        .await
        .expect("stream consume");
    assert_eq!(acc.content, "Hello world");
    assert!(acc.saw_content);
    let usage = acc.usage.expect("usage chunk");
    assert_eq!(usage.prompt_tokens, Some(10));
    assert_eq!(usage.completion_tokens, Some(5));
    assert_eq!(usage.total_tokens, Some(15));
}

#[tokio::test]
async fn merges_tool_call_fragments_by_index() {
    let line_a = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":"{\"city\":"}}]}}]}"#;
    let line_b = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_2","function":{"name":"now","arguments":"{}"}}]}}]}"#;
    let line_c = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"SF\"}"}}]}}]}"#;
    // 结尾必须带 [DONE]：流终止校验要求「正常结束」留下证据
    let payload = format!("{line_a}\n\n{line_b}\n\n{line_c}\n\ndata: [DONE]\n");

    let acc = consume_think_stream(stream_of(vec![payload]), IDLE)
        .await
        .expect("stream consume");
    let mut calls = acc.tool_calls.into_values();
    let first = calls.next().expect("tool call 0");
    assert_eq!(first.id, "call_1");
    assert_eq!(first.name, "get_weather");
    assert_eq!(first.arguments, "{\"city\":\"SF\"}");
    let second = calls.next().expect("tool call 1");
    assert_eq!(second.id, "call_2");
    assert_eq!(second.name, "now");
    assert_eq!(second.arguments, "{}");
    assert!(calls.next().is_none());
}

#[tokio::test]
async fn idle_timeout_fires_when_stream_stalls() {
    let err = consume_think_stream(
        futures_util::stream::pending::<reqwest::Result<Vec<u8>>>(),
        Duration::from_millis(20),
    )
    .await
    .expect_err("stalled stream must time out");
    assert!(matches!(err.code, ErrorCode::Internal));
    assert!(err.msg.contains("idle timeout"), "msg: {}", err.msg);
}

#[tokio::test]
async fn malformed_sse_data_is_strict_error() {
    let err = consume_think_stream(stream_of(vec!["data: {not-json}\n".to_string()]), IDLE)
        .await
        .expect_err("malformed data must fail");
    assert!(
        err.msg.contains("sse chunk parse failed"),
        "msg: {}",
        err.msg
    );
}

#[test]
fn parses_sse_line_variants() {
    assert!(parse_sse_line("").unwrap().is_none());
    assert!(parse_sse_line(": keep-alive").unwrap().is_none());
    assert!(parse_sse_line("event: ping").unwrap().is_none());
    assert!(matches!(
        parse_sse_line("data: [DONE]").unwrap(),
        Some(SseEvent::Done)
    ));
    let event = parse_sse_line(r#"data: {"choices":[{"delta":{"content":"hi"}}]}"#)
        .unwrap()
        .expect("data line parses to an event");
    let SseEvent::Chunk(chunk) = event else {
        panic!("expected chunk event");
    };
    assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("hi"));
}

#[tokio::test]
async fn finish_maps_tool_calls_with_content_and_usage() {
    let ctx = test_ctx();
    let mut acc = StreamAccumulator::default();
    acc.absorb(chunk_of(
        r#"{"choices":[{"delta":{"content":"thinking..."}}]}"#,
    ));
    acc.absorb(chunk_of(
        r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"t","arguments":"{\"a\":1}"}}]}}],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}}"#,
    ));
    match finish_think_result(&ctx, acc) {
        ThinkResult::ToolCall {
            content,
            tool_calls,
            usage,
        } => {
            assert_eq!(content.as_deref(), Some("thinking..."));
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].id, "c1");
            assert_eq!(tool_calls[0].name, "t");
            assert_eq!(tool_calls[0].arguments, serde_json::json!({"a": 1}));
            assert_eq!(usage.input_tokens, 3);
            assert_eq!(usage.output_tokens, 4);
            assert_eq!(usage.total_tokens, Some(7));
        }
        other => panic!("expected tool call result, got {other:?}"),
    }
}

#[tokio::test]
async fn finish_maps_final_answer_with_default_usage() {
    let ctx = test_ctx();
    let mut acc = StreamAccumulator::default();
    acc.absorb(chunk_of(r#"{"choices":[{"delta":{"content":"hi"}}]}"#));
    match finish_think_result(&ctx, acc) {
        ThinkResult::Final { content, usage } => {
            assert_eq!(content, "hi");
            assert_eq!(usage.input_tokens, 0);
            assert_eq!(usage.output_tokens, 0);
            assert_eq!(usage.total_tokens, None);
        }
        other => panic!("expected final result, got {other:?}"),
    }
}

// ==================== 流终止校验（静默截断 → 显式错误） ====================

#[tokio::test]
async fn stream_without_termination_evidence_is_error() {
    // 既没有 `[DONE]` 也没有 finish_reason —— 连接在生成中途被切断
    let payload = "data: {\"choices\":[{\"delta\":{\"content\":\"half\"}}]}\n".to_string();
    let err = consume_think_stream(stream_of(vec![payload]), IDLE)
        .await
        .expect_err("stream without termination evidence must fail");
    assert!(matches!(err.code, ErrorCode::Internal));
    assert!(err.msg.contains("ended unexpectedly"), "msg: {}", err.msg);
}

#[tokio::test]
async fn finish_reason_alone_counts_as_clean_end() {
    // 部分网关不发 `[DONE]`，但正常结束的 chunk 会带 finish_reason → 视为正常
    let payload =
        "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n"
            .to_string();
    let acc = consume_think_stream(stream_of(vec![payload]), IDLE)
        .await
        .expect("finish_reason is enough evidence of a clean end");
    assert_eq!(acc.content, "hi");
}

#[tokio::test]
async fn stream_truncated_by_max_tokens_is_error() {
    // max_tokens 截断：此时 arguments 必为半截 JSON，必须显式失败而不是降级成 null 参数
    let payload = concat!(
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":"#,
        r#"{"name":"create_task","arguments":"{\"title\":\"半截"}}]},"finish_reason":"length"}]}"#,
        "\n"
    )
    .to_string();
    let err = consume_think_stream(stream_of(vec![payload]), IDLE)
        .await
        .expect_err("truncated stream must fail");
    assert!(matches!(err.code, ErrorCode::Internal));
    assert!(
        err.msg.contains("truncated by max_tokens"),
        "msg: {}",
        err.msg
    );
    assert!(err.msg.contains("create_task"), "msg: {}", err.msg);
}

#[tokio::test]
async fn index_less_deltas_do_not_merge_parallel_calls() {
    // provider 省略 index 时，第二路调用的 fragment 不能被并进 index=0
    // （并进去会把两次调用的 arguments 拼成 `{...}{...}`，非法 JSON → 参数整包变 null）
    let first = r#"data: {"choices":[{"delta":{"tool_calls":[{"id":"call_a","function":{"name":"a","arguments":"{\"x\":1}"}}]}}]}"#;
    let second = r#"data: {"choices":[{"delta":{"tool_calls":[{"id":"call_b","function":{"name":"b","arguments":"{\"y\":2}"}}]}}]}"#;
    let payload = format!("{first}\n\n{second}\n\ndata: [DONE]\n");

    let acc = consume_think_stream(stream_of(vec![payload]), IDLE)
        .await
        .expect("stream consume");
    assert!(
        acc.saw_index_less_tool_call,
        "missing index should be recorded for logging"
    );
    let mut calls = acc.tool_calls.into_values();
    let a = calls.next().expect("tool call a");
    assert_eq!(
        (a.id.as_str(), a.name.as_str(), a.arguments.as_str()),
        ("call_a", "a", "{\"x\":1}")
    );
    let b = calls.next().expect("tool call b");
    assert_eq!(
        (b.id.as_str(), b.name.as_str(), b.arguments.as_str()),
        ("call_b", "b", "{\"y\":2}")
    );
    assert!(calls.next().is_none(), "must not be merged into one slot");
}

#[test]
fn index_less_deltas_of_same_call_still_merge() {
    // 同一路调用的后续 fragment（带 id、无 index）仍要并进原槽位
    let mut acc = StreamAccumulator::default();
    acc.absorb(chunk_of(
        r#"{"choices":[{"delta":{"tool_calls":[{"id":"c1","function":{"name":"t","arguments":"{\"city\":"}}]}}]}"#,
    ));
    acc.absorb(chunk_of(
        r#"{"choices":[{"delta":{"tool_calls":[{"id":"c1","function":{"arguments":"\"SF\"}"}}]}}]}"#,
    ));
    let calls: Vec<_> = acc.tool_calls.into_values().collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments, "{\"city\":\"SF\"}");
}

#[tokio::test]
async fn invalid_tool_arguments_degrade_to_null_and_keep_tool_identity() {
    // 坏 JSON 仍降级为 null（保留容错，让模型有机会自我纠正），
    // 但原文会经 `cortex_stream` 日志留痕，不再静默丢弃
    let ctx = test_ctx();
    let mut acc = StreamAccumulator::default();
    acc.absorb(chunk_of(
        r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"t","arguments":"{oops"}}]}}]}"#,
    ));
    match finish_think_result(&ctx, acc) {
        ThinkResult::ToolCall { tool_calls, .. } => {
            assert_eq!(tool_calls[0].arguments, Value::Null);
            assert_eq!(tool_calls[0].name, "t");
            assert_eq!(tool_calls[0].id, "c1");
        }
        other => panic!("expected tool call result, got {other:?}"),
    }
}

#[test]
fn truncate_for_log_keeps_head_and_tail() {
    let out = truncate_for_log(&"A".repeat(5000));
    assert!(out.len() < 5000, "long text must be truncated");
    assert!(out.contains("omitted"), "out: {out}");

    // JSON 语法错误多在尾部，尾段必须保留
    let text = format!("{}TAIL_MARKER", "A".repeat(4900));
    let out = truncate_for_log(&text);
    assert!(out.ends_with("TAIL_MARKER"), "tail must survive: {out}");

    // 短文本原样返回
    assert_eq!(truncate_for_log("{\"a\":1}"), "{\"a\":1}");
}

// ==================== 访问模式分流（方案 §四） ====================

fn provider_with_config(config: &str) -> ModelProviderPo {
    ModelProviderPo {
        id: "prov_1".to_string(),
        name: "test-provider".to_string(),
        provider_type: common::enums::ProviderType::OpenAI,
        model_name: "gpt-4o".to_string(),
        capability: common::enums::ModelCapability::Agent,
        api_key: "sk-test".to_string(),
        base_url: None,
        description: None,
        config: config.to_string(),
        status: common::enums::ModelProviderStatus::Normal,
        created_by: "tester".to_string(),
        modified_by: "tester".to_string(),
        created_at: 0,
        updated_at: 0,
    }
}

#[test]
fn stream_body_keeps_stream_fields() {
    let provider = provider_with_config("{}");
    let body = build_chat_request_body(&provider, &[], &[], ModelAccessMode::Stream);
    assert_eq!(body["stream"], serde_json::json!(true));
    assert_eq!(
        body["stream_options"]["include_usage"],
        serde_json::json!(true)
    );
    assert_eq!(body["model"], serde_json::json!("gpt-4o"));
}

#[test]
fn non_stream_body_omits_stream_fields_and_keeps_tools() {
    let provider = provider_with_config("{}");
    let body = build_chat_request_body(&provider, &[], &[], ModelAccessMode::NonStream);
    assert!(
        body.get("stream").is_none(),
        "non_stream 请求体不得携带 stream 字段: {body}"
    );
    assert!(
        body.get("stream_options").is_none(),
        "non_stream 请求体不得携带 stream_options 字段: {body}"
    );
    assert_eq!(body["model"], serde_json::json!("gpt-4o"));

    // tools 两路一致：非空时照常携带
    let tools = [ToolDescriptor {
        name: "t".to_string(),
        description: "d".to_string(),
        parameters: serde_json::json!({"type": "object"}),
    }];
    let body = build_chat_request_body(&provider, &[], &tools, ModelAccessMode::NonStream);
    assert!(
        body.get("tools").is_some(),
        "tools 不得因访问模式丢失: {body}"
    );
}

#[tokio::test]
async fn resolve_access_mode_defaults_to_stream_and_falls_back_on_dirty_config() {
    let ctx = test_ctx();
    // 缺省（{}）→ Stream（存量行为零变化）
    assert_eq!(
        resolve_access_mode(&ctx, &provider_with_config("{}")),
        ModelAccessMode::Stream
    );
    // 脏 config（JSON 解析失败）→ 兜底 Stream + 告警日志（不中断推理）
    assert_eq!(
        resolve_access_mode(&ctx, &provider_with_config("{not-json")),
        ModelAccessMode::Stream
    );
    // 显式 non_stream → NonStream
    assert_eq!(
        resolve_access_mode(
            &ctx,
            &provider_with_config(r#"{"access_mode":"non_stream"}"#)
        ),
        ModelAccessMode::NonStream
    );
}

#[tokio::test]
async fn parses_non_stream_body_with_content_tool_calls_and_usage() {
    let body = r#"{
        "choices": [{
            "message": {
                "content": "let me check",
                "tool_calls": [
                    {"id": "call_1", "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}},
                    {"id": "call_2", "function": {"name": "now", "arguments": "{}"}}
                ]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {"prompt_tokens": 11, "completion_tokens": 7, "total_tokens": 18}
    }"#;
    let acc = parse_non_stream_body(body).expect("non-stream body parses");
    assert!(acc.saw_content);
    assert_eq!(acc.content, "let me check");
    assert_eq!(acc.finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(acc.tool_calls.len(), 2);

    // 两路统一经 finish_think_result 组装
    let ctx = test_ctx();
    match finish_think_result(&ctx, acc) {
        ThinkResult::ToolCall {
            content,
            tool_calls,
            usage,
        } => {
            assert_eq!(content.as_deref(), Some("let me check"));
            assert_eq!(tool_calls[0].id, "call_1");
            assert_eq!(tool_calls[0].name, "get_weather");
            assert_eq!(tool_calls[0].arguments, serde_json::json!({"city": "SF"}));
            assert_eq!(tool_calls[1].id, "call_2");
            assert_eq!(usage.input_tokens, 11);
            assert_eq!(usage.output_tokens, 7);
            assert_eq!(usage.total_tokens, Some(18));
        }
        other => panic!("expected tool call result, got {other:?}"),
    }
}

#[test]
fn non_stream_body_missing_finish_reason_is_error() {
    let body = r#"{"choices":[{"message":{"content":"half"}}]}"#;
    let err = parse_non_stream_body(body).expect_err("missing finish_reason must fail");
    assert!(
        err.msg.contains("missing finish_reason"),
        "msg: {}",
        err.msg
    );
}

#[test]
fn non_stream_body_truncated_by_max_tokens_is_error() {
    // finish_reason=length：与流式同源复用 check_stream_end 的截断防护
    let body = r#"{"choices":[{"message":{"tool_calls":[{"id":"c1","function":{"name":"create_task","arguments":"{\"title\":"}}]},"finish_reason":"length"}]}"#;
    let err = parse_non_stream_body(body).expect_err("max_tokens truncation must fail");
    assert!(
        err.msg.contains("truncated by max_tokens"),
        "msg: {}",
        err.msg
    );
    assert!(err.msg.contains("create_task"), "msg: {}", err.msg);
}

#[test]
fn non_stream_body_parse_failure_is_error() {
    let err = parse_non_stream_body("this is not json").expect_err("bad json must fail");
    assert!(
        err.msg.contains("non-stream response parse failed"),
        "msg: {}",
        err.msg
    );
}

#[test]
fn non_stream_body_without_choices_is_error() {
    // choices 缺失（serde default 空数组）→ 异常响应显式失败
    let err =
        parse_non_stream_body(r#"{"object":"chat.completion"}"#).expect_err("no choices must fail");
    assert!(err.msg.contains("no choices"), "msg: {}", err.msg);
}
