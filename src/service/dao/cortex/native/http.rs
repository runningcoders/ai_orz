//! HTTP 辅助函数 - 直接调用 OpenAI 兼容 API
//!
//! 所有 provider 统一走 OpenAI Chat Completions / Embeddings 协议，
//! 不再依赖 rig 的 client/agent 抽象。

use crate::models::cortex_types::{
    ChatMessage, ThinkResult, TokenUsage, ToolCallRequest, ToolDescriptor,
};
use crate::models::model_provider::{ModelProviderConfig, ModelProviderPo};
use crate::pkg::RequestContext;
use crate::pkg::http::presets;
use common::enums::ModelAccessMode;
use common::error::{Error, ErrorCode, Result, err};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

/// 解析 base_url：provider 配置优先于默认值
pub fn resolve_base_url(provider: &ModelProviderPo, default: &str) -> String {
    provider
        .base_url
        .clone()
        .unwrap_or_else(|| default.to_string())
}

/// 根据 provider_type 获取默认 base_url
pub fn default_base_url(provider_type: common::enums::ProviderType) -> &'static str {
    match provider_type {
        common::enums::ProviderType::OpenAI => "https://api.openai.com/v1",
        common::enums::ProviderType::DeepSeek => "https://api.deepseek.com",
        common::enums::ProviderType::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1",
        common::enums::ProviderType::Doubao | common::enums::ProviderType::DoubaoVision => {
            "https://ark.cn-beijing.volces.com/api/v3"
        }
        common::enums::ProviderType::Ollama => "http://localhost:11434/v1",
        common::enums::ProviderType::Custom => "",
        common::enums::ProviderType::FastEmbed => "",
    }
}

/// 校验 provider 出站调用前置条件（HTTP 请求前的快速失败）
///
/// - `api_key` 为空：硬错误。OpenAI 兼容协议必须携带鉴权，空 key 发出去只会被服务端拒绝，
///   在此直接返回避免无意义的网络往返与超时挂起。
/// - `base_url`：非强依赖，可由 `provider_type` 推理默认值（见 `default_base_url`），
///   故此处不校验；若 `Custom`/`FastEmbed` 等无默认值的类型未配置 base_url，
///   解析后为空会在后续 URL 拼接时自然失败。
pub fn validate_provider_for_request(provider: &ModelProviderPo) -> Result<()> {
    if provider.api_key.trim().is_empty() {
        return Err(err!(
            ConfigInvalid,
            "model provider api_key is empty, cannot call model API"
        ));
    }
    Ok(())
}

/// 将 ChatMessage 数组转换为 OpenAI API 格式的 JSON 数组
fn messages_to_json(messages: &[ChatMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| match m {
            ChatMessage::System { content } => json!({"role": "system", "content": content}),
            ChatMessage::User { content } => json!({"role": "user", "content": content}),
            ChatMessage::Assistant {
                content,
                tool_calls,
            } => {
                let mut msg = json!({"role": "assistant"});
                if let Some(c) = content {
                    msg["content"] = json!(c);
                } else {
                    msg["content"] = json!(null);
                }
                if let Some(tcs) = tool_calls {
                    msg["tool_calls"] = json!(
                        tcs.iter()
                            .map(|tc| {
                                json!({
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.name,
                                        "arguments": tc.arguments.to_string(),
                                    }
                                })
                            })
                            .collect::<Vec<_>>()
                    );
                }
                msg
            }
            ChatMessage::Tool {
                tool_call_id,
                content,
            } => {
                json!({"role": "tool", "tool_call_id": tool_call_id, "content": content})
            }
        })
        .collect()
}

/// 调用 Chat Completions API
///
/// 所有 provider 统一走 /chat/completions endpoint。访问模式按 provider 配置分流
/// （方案 §四，缺省/脏 config 兜底 stream）：
/// - stream：请求侧启用 SSE 流式，超时判定从「请求总时长」改为「chunk 间隔空闲检测 +
///   总时长硬上限兜底」，流式聚合细节见 [`consume_think_stream`]；
/// - non_stream：请求体不含流式字段，一次性解析完整 JSON 响应，仅总时长硬上限兜底。
///
/// 两路统一经 [`finish_think_result`] 组装上层 [`ThinkResult`]。
pub async fn call_chat_completions(
    ctx: RequestContext,
    client: &reqwest::Client,
    provider: &ModelProviderPo,
    messages: &[ChatMessage],
    tools: &[ToolDescriptor],
) -> Result<ThinkResult> {
    validate_provider_for_request(provider)?;
    let base_url = resolve_base_url(provider, default_base_url(provider.provider_type));
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    // 方案 §四：按配置解析访问模式（缺省/脏 config 兜底 stream + 告警日志，不中断推理）
    let access_mode = resolve_access_mode(&ctx, provider);

    // 构建请求体：按访问模式分流 stream 字段，model/messages/tools 两路一致
    let body = build_chat_request_body(provider, messages, tools, access_mode);

    log_debug!(
        ctx,
        "cortex_chat_request",
        "POST {} model={} messages={}",
        url,
        provider.model_name,
        messages.len()
    );

    let resp = client
        .post(&url)
        .bearer_auth(&provider.api_key)
        .timeout(presets::LLM_STREAM_TOTAL_TIMEOUT)
        .json(&body)
        .send()
        .await
        .map_err(|e| transport_error("chat completions", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp
            .text()
            .await
            .unwrap_or_else(|_| "<no body>".to_string());
        return Err(model_call_error("chat completions", status, &text));
    }

    // 方案 §四：按访问模式分流响应消费；两路统一经 finish_think_result 组装
    let acc = match access_mode {
        ModelAccessMode::Stream => {
            consume_think_stream(resp.bytes_stream(), presets::LLM_STREAM_IDLE_TIMEOUT).await?
        }
        ModelAccessMode::NonStream => parse_non_stream_response(resp).await?,
    };
    Ok(finish_think_result(&ctx, acc))
}

/// 解析 provider 的下行调用访问模式（方案 §四）。
///
/// - 配置缺省（None）→ [`ModelAccessMode::Stream`]（= 存量行为，零变化）；
/// - config JSON 解析失败（脏配置）→ 兜底 Stream + 告警日志，不中断推理。
fn resolve_access_mode(ctx: &RequestContext, provider: &ModelProviderPo) -> ModelAccessMode {
    match serde_json::from_str::<ModelProviderConfig>(&provider.config) {
        Ok(cfg) => cfg.access_mode_or_default(),
        Err(e) => {
            log_warn!(
                ctx,
                "cortex_chat_request",
                "provider config parse failed, falling back to stream access mode: provider_id={} error={}",
                provider.id,
                e
            );
            ModelAccessMode::Stream
        }
    }
}

/// 构建 Chat Completions 请求体（方案 §四：按访问模式分流 stream 字段）。
///
/// model/messages/tools 两路一致；stream 模式附加 `stream` + `stream_options`
/// （include_usage 让 usage 随最后一个 chunk 下发），non_stream 模式不含任何流式字段。
fn build_chat_request_body(
    provider: &ModelProviderPo,
    messages: &[ChatMessage],
    tools: &[ToolDescriptor],
    access_mode: ModelAccessMode,
) -> Value {
    let mut body = json!({
        "model": provider.model_name,
        "messages": messages_to_json(messages),
    });
    if access_mode == ModelAccessMode::Stream {
        body["stream"] = json!(true);
        body["stream_options"] = json!({ "include_usage": true });
    }
    // 如果有工具，添加 tools 字段
    if !tools.is_empty() {
        body["tools"] = json!(
            tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect::<Vec<_>>()
        );
    }
    body
}

/// 读取并解析非流式 Chat Completions 响应体（方案 §四）。
async fn parse_non_stream_response(resp: reqwest::Response) -> Result<StreamAccumulator> {
    let text = resp
        .text()
        .await
        .map_err(|e| transport_error("chat completions body", e))?;
    parse_non_stream_body(&text)
}

/// 解析非流式响应体为与流式聚合同构的 [`StreamAccumulator`]（复用 [`finish_think_result`] 组装）。
///
/// 截断防护（方案 §四）：正常结束必须携带 `finish_reason`，缺失视为响应被网关/代理
/// 截断——半截 content/arguments 不得当成功返回；`finish_reason=length` 走
/// [`check_stream_end`] 与流式同源的 max_tokens 截断防护。
fn parse_non_stream_body(text: &str) -> Result<StreamAccumulator> {
    let body: NonStreamResponse = serde_json::from_str(text).map_err(|e| {
        err!(
            Internal,
            "chat completions non-stream response parse failed: {e}; body={}",
            truncate_for_log(text)
        )
    })?;

    let Some(choice) = body.choices.into_iter().next() else {
        return Err(err!(
            Internal,
            "chat completions non-stream response has no choices; body={}",
            truncate_for_log(text)
        ));
    };

    let mut acc = StreamAccumulator {
        usage: body.usage,
        ..Default::default()
    };
    if let Some(message) = choice.message {
        if let Some(content) = message.content {
            acc.saw_content = true;
            acc.content = content;
        }
        // 非流式 tool_calls 是完整数组，按数组顺序落槽（无跨 chunk 消歧问题）
        for tc in message.tool_calls.into_iter().flatten() {
            let index = acc.tool_calls.len();
            let entry = acc.tool_calls.entry(index).or_default();
            if let Some(id) = tc.id {
                entry.id = id;
            }
            if let Some(func) = tc.function {
                if let Some(name) = func.name {
                    entry.name.push_str(&name);
                }
                if let Some(args) = func.arguments {
                    entry.arguments.push_str(&args);
                }
            }
        }
    }

    if choice.finish_reason.is_none() {
        return Err(err!(
            Internal,
            "chat completions non-stream response missing finish_reason \
             (possibly truncated by gateway), content_len={} tool_calls={}",
            acc.content.len(),
            acc.tool_calls.len()
        ));
    }
    acc.finish_reason = choice.finish_reason;

    // 与流式同源：finish_reason=length（max_tokens 截断）在此显式失败
    check_stream_end(&acc)?;
    Ok(acc)
}

/// 将流式聚合结果组装为上层 [`ThinkResult`]（与原非流式解析约定一致）。
///
/// 需要 `ctx` 仅为留痕：模型给出的 `arguments` 若不是合法 JSON，原文必须进日志
/// （否则下游只能看到「参数整包为 null」，病因被永久掩盖，详见本函数内注释）。
fn finish_think_result(ctx: &RequestContext, acc: StreamAccumulator) -> ThinkResult {
    let usage = TokenUsage {
        input_tokens: acc
            .usage
            .as_ref()
            .and_then(|u| u.prompt_tokens)
            .unwrap_or(0),
        output_tokens: acc
            .usage
            .as_ref()
            .and_then(|u| u.completion_tokens)
            .unwrap_or(0),
        total_tokens: acc.usage.as_ref().and_then(|u| u.total_tokens),
    };

    if acc.saw_index_less_tool_call {
        log_warn!(
            ctx,
            "cortex_stream",
            "model stream omitted `index` on tool_call deltas; slots resolved by fallback rule, tool_calls={}",
            acc.tool_calls.len()
        );
    }

    if !acc.tool_calls.is_empty() {
        let calls: Vec<ToolCallRequest> = acc
            .tool_calls
            .into_values()
            .map(|tc| ToolCallRequest {
                arguments: parse_tool_arguments(ctx, &tc),
                id: tc.id,
                name: tc.name,
            })
            .collect();
        return ThinkResult::ToolCall {
            content: acc.saw_content.then_some(acc.content),
            tool_calls: calls,
            usage,
        };
    }

    ThinkResult::Final {
        content: acc.content,
        usage,
    }
}

/// 解析模型给出的 tool_call 参数原文。
///
/// 解析失败时**先把原文写进日志**再降级为 `Value::Null`。这条日志是唯一能指认病因的证据：
/// 降级后下游（`handler_adapter`）只能报 `invalid type: null, expected struct XxxRequest`，
/// 病症指向「某个参数取值不对」，而真正的原因（模型写坏 JSON / 被 max_tokens 截断 /
/// 同轮多次调用的 arguments 被拼成一段）此前是**静默丢弃**的，靠日志永远查不出来。
fn parse_tool_arguments(ctx: &RequestContext, tc: &AccumToolCall) -> Value {
    match serde_json::from_str::<Value>(&tc.arguments) {
        Ok(value) => value,
        Err(e) => {
            log_error!(
                ctx,
                "cortex_stream",
                "tool call arguments is not valid JSON, degrading to null: tool={} call_id={} error={} raw_args={}",
                tc.name,
                tc.id,
                e,
                truncate_for_log(&tc.arguments)
            );
            Value::Null
        }
    }
}

/// 日志友好的长文本：超长时保留首尾两段。
///
/// 只留头部会丢掉语法错误位置——坏 JSON 的报错（`EOF while parsing`、未闭合括号、
/// 尾逗号）几乎总在**尾部**。
fn truncate_for_log(text: &str) -> String {
    const HEAD: usize = 1200;
    const TAIL: usize = 600;
    if text.len() <= HEAD + TAIL {
        return text.to_string();
    }
    let head_end = floor_char_boundary(text, HEAD);
    let tail_start = ceil_char_boundary(text, text.len() - TAIL);
    format!(
        "{} …<omitted {} bytes>… {}",
        &text[..head_end],
        tail_start - head_end,
        &text[tail_start..]
    )
}

/// 向下取最近的 UTF-8 字符边界（避免切断多字节字符）
fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// 向上取最近的 UTF-8 字符边界
fn ceil_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// 消费 SSE 响应流并聚合为 [`StreamAccumulator`]。
///
/// 超时判定（替代原「请求总时长」）：
/// - **空闲超时**：相邻 chunk 的最大间隔超过 `idle_timeout`（tokio 层逐 next 计时），
///   判定流中断返回错误。只要 token 持续产出即视为正常，长生成不再被总时长误杀。
/// - **总时长硬上限**：由请求构建处的逐请求 `.timeout()` 兜底（见调用方），
///   防服务端异常（如无限心跳）导致空闲判定永不触发。
///
/// 终止校验（`check_stream_end`）：流「正常结束」必须留下证据，否则一律判为截断——
/// 原实现 `Ok(None) => break` 后直接返回 `Ok`，连接被切断时半截结果会被当成功使用。
async fn consume_think_stream<S, B>(stream: S, idle_timeout: Duration) -> Result<StreamAccumulator>
where
    S: Stream<Item = reqwest::Result<B>>,
    B: AsRef<[u8]>,
{
    let mut stream = Box::pin(stream);
    let mut acc = StreamAccumulator::default();
    let mut buf: Vec<u8> = Vec::new();
    'stream: loop {
        let chunk = match tokio::time::timeout(idle_timeout, stream.next()).await {
            Ok(None) => break,
            Err(_) => {
                return Err(err!(
                    Internal,
                    "chat completions stream idle timeout: no data for {idle_timeout:?}"
                ));
            }
            Ok(Some(item)) => item.map_err(|e| transport_error("chat completions stream", e))?,
        };
        buf.extend_from_slice(chunk.as_ref());
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            if let Some(event) = parse_sse_line(&line)? {
                match event {
                    // `[DONE]` 之后的内容一律丢弃，故直接跳出到终止校验
                    SseEvent::Done => {
                        acc.saw_done = true;
                        break 'stream;
                    }
                    SseEvent::Chunk(c) => acc.absorb(c),
                }
            }
        }
    }
    // 容忍服务端未以换行收尾的最后一行
    if !acc.saw_done && !buf.is_empty() {
        let line = String::from_utf8_lossy(&buf);
        if let Some(event) = parse_sse_line(&line)? {
            match event {
                SseEvent::Done => acc.saw_done = true,
                SseEvent::Chunk(c) => acc.absorb(c),
            }
        }
    }
    check_stream_end(&acc)?;
    Ok(acc)
}

/// 流终止校验：把「静默的截断」变成显式错误。
///
/// 1. **结束证据缺失**：正常结束必然留下 `[DONE]` 或带 `finish_reason` 的 chunk（二者之一）。
///    两者皆无说明连接在生成中途被切断——半截 `content` / 半截 `arguments` 此前会被
///    当成功返回，是本文件里最隐蔽的一条静默通道。
/// 2. **`finish_reason=length`**：被 `max_tokens` 截断。此时 `arguments` 一定是半截 JSON，
///    经 [`parse_tool_arguments`] 降级成 `null` 后，下游只会报「参数整包为 null」这个
///    与病因无关的错；在此直接失败，并把截断现场的规模信息带出去。
fn check_stream_end(acc: &StreamAccumulator) -> Result<()> {
    if !acc.saw_done && acc.finish_reason.is_none() {
        return Err(err!(
            Internal,
            "chat completions stream ended unexpectedly: no `[DONE]` and no finish_reason \
             (connection cut mid-stream), content_len={}",
            acc.content.len()
        ));
    }
    if acc.finish_reason.as_deref() == Some("length") {
        let pending: Vec<&str> = acc.tool_calls.values().map(|t| t.name.as_str()).collect();
        return Err(err!(
            Internal,
            "chat completions truncated by max_tokens (finish_reason=length): content_len={} pending_tool_calls={:?}",
            acc.content.len(),
            pending
        ));
    }
    Ok(())
}

/// 调用标准 Embeddings API
pub async fn call_embeddings(
    client: &reqwest::Client,
    provider: &ModelProviderPo,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    validate_provider_for_request(provider)?;
    let base_url = resolve_base_url(provider, default_base_url(provider.provider_type));
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));

    let body = json!({
        "model": provider.model_name,
        "input": texts,
    });

    let resp = client
        .post(&url)
        .bearer_auth(&provider.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| transport_error("embeddings", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp
            .text()
            .await
            .unwrap_or_else(|_| "<no body>".to_string());
        return Err(model_call_error("embeddings", status, &text));
    }

    let resp_body: EmbeddingResponse = resp
        .json()
        .await
        .map_err(|e| err!(Internal, "embeddings response parse failed: {}", e))?;

    Ok(resp_body
        .data
        .into_iter()
        .map(|d| d.embedding.into_iter().map(|x| x as f32).collect())
        .collect())
}

/// 调用豆包 Vision 多模态 Embeddings API
///
/// DoubaoVision 使用 /embeddings/multimodal endpoint，与标准 /embeddings 不兼容：
/// - 请求体 input 是 [{type:"text", text:"..."}] 而非字符串数组
/// - 响应体 data.embedding（单对象）而非 data[0].embedding
/// - 多文本必须逐条请求（multimodal endpoint 会融合成一个 embedding）
pub async fn call_embeddings_multimodal(
    client: &reqwest::Client,
    provider: &ModelProviderPo,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    validate_provider_for_request(provider)?;
    let base_url = resolve_base_url(provider, default_base_url(provider.provider_type));
    let url = format!("{}/embeddings/multimodal", base_url.trim_end_matches('/'));

    let mut results = Vec::with_capacity(texts.len());
    for text in texts {
        let body = json!({
            "model": provider.model_name,
            "input": [{"type": "text", "text": text}],
        });

        let resp = client
            .post(&url)
            .bearer_auth(&provider.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| transport_error("DoubaoVision embedding", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "<no body>".to_string());
            return Err(model_call_error("DoubaoVision embeddings", status, &text));
        }

        let resp_body: MultimodalEmbeddingResponse = resp.json().await.map_err(|e| {
            err!(
                Internal,
                "DoubaoVision embedding response parse failed: {}",
                e
            )
        })?;

        results.push(
            resp_body
                .data
                .embedding
                .into_iter()
                .map(|x| x as f32)
                .collect(),
        );
    }

    Ok(results)
}

// ==================== 错误分类 ====================

/// 传输阶段（发送请求 / 读取响应流）的错误分类与根因提取。
///
/// reqwest 顶层 Display 只有 "error sending request for url (...)" 这类笼统文案，
/// 真实原因（超时 / 连接失败等）在 error source 链上；这里显式打标并追根，
/// 避免日志只见 URL 不见原因。保持 Internal 错误码：传输失败是否可重试
/// 仍由上层策略裁决，不在分类处改变语义。
fn transport_error(kind: &str, e: reqwest::Error) -> Error {
    let tag = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else {
        "send"
    };
    let root = error_root_cause(&e);
    let root = if root.is_empty() { "<no cause>" } else { &root };
    err!(
        Internal,
        "{kind} request failed ({tag}): {e}; root cause: {root}"
    )
}

/// 沿 error source 链走到最底层，取根因文本。
fn error_root_cause(e: &reqwest::Error) -> String {
    let mut current: &dyn std::error::Error = e;
    while let Some(source) = current.source() {
        current = source;
    }
    current.to_string()
}

/// 将模型 HTTP 调用的非成功状态码映射为具体的模型错误码。
///
/// 区分可重试（限流/服务端）与不可重试（鉴权/请求非法/内容过滤）错误，
/// 供上游业务层（如 AOP 消费者）按需调用 `Error::is_retryable()` 决定是否重试。
fn model_call_error(kind: &str, status: reqwest::StatusCode, text: &str) -> Error {
    let detail = format!("{kind} failed ({}): {}", status, text);
    match status.as_u16() {
        // 限流：可重试
        429 => Error::new(
            ErrorCode::ModelRateLimited,
            format!("{kind} rate limited (429): {text}"),
        ),
        // 鉴权失败：不可重试
        401 | 403 => Error::new(ErrorCode::ModelAuth, detail),
        // 客户端错误：默认请求非法，但若响应体表明是内容过滤则单独归类
        400..=499 => {
            if is_content_filtered(text) {
                Error::new(ErrorCode::ModelContentFiltered, detail)
            } else {
                Error::new(ErrorCode::ModelBadRequest, detail)
            }
        }
        // 服务端/网关错误：可重试
        500..=599 => Error::new(ErrorCode::ModelServerError, detail),
        // 其它未归类状态码：按服务端错误对待（可重试）
        _ => Error::new(ErrorCode::ModelServerError, detail),
    }
}

/// 粗略判断响应体是否为内容过滤（moderation）类错误。
fn is_content_filtered(text: &str) -> bool {
    text.contains("content_filter")
        || text.contains("moderation")
        || text.contains("\"code\":\"content_filter\"")
        || text.contains("\"type\":\"content_filter\"")
}

// ==================== 流式聚合结构 ====================

/// SSE 流的聚合态：content 按 delta 累加、tool_calls 按 index 跨 chunk 合并
#[derive(Debug, Default)]
struct StreamAccumulator {
    content: String,
    // 区分「无内容」与「空字符串」（ToolCall.content 的 None/"" 语义对齐非流式）
    saw_content: bool,
    /// 是否收到 `[DONE]` 终止标记（流正常结束的证据之一，见 [`check_stream_end`]）
    saw_done: bool,
    /// 最后一次 outcome 标记：`stop` / `length` / `tool_calls` / `content_filter` 等
    finish_reason: Option<String>,
    /// 是否出现过「未带 index」的 tool_call delta（provider 不规范，需消歧，见 [`StreamAccumulator::resolve_tool_call_index`]）
    saw_index_less_tool_call: bool,
    tool_calls: BTreeMap<usize, AccumToolCall>,
    usage: Option<Usage>,
}

#[derive(Debug, Default)]
struct AccumToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl StreamAccumulator {
    fn absorb(&mut self, chunk: StreamChunk) {
        if chunk.usage.is_some() {
            self.usage = chunk.usage;
        }
        for choice in chunk.choices {
            // finish_reason 只在流末尾的某个 chunk 出现，记录最后一次即最终原因
            if let Some(reason) = choice.finish_reason {
                self.finish_reason = Some(reason);
            }
            if let Some(c) = choice.delta.content {
                self.saw_content = true;
                self.content.push_str(&c);
            }
            for tc in choice.delta.tool_calls.into_iter().flatten() {
                let index = self.resolve_tool_call_index(&tc);
                let entry = self.tool_calls.entry(index).or_default();
                if let Some(id) = tc.id {
                    entry.id = id;
                }
                if let Some(func) = tc.function {
                    if let Some(name) = func.name {
                        entry.name.push_str(&name);
                    }
                    if let Some(args) = func.arguments {
                        entry.arguments.push_str(&args);
                    }
                }
            }
        }
    }

    /// 解析该 delta 归属的 tool_call 槽位。
    ///
    /// OpenAI 流式协议里 `index` 是**分组依据**，但部分 provider 只在首个 fragment 带、
    /// 并行调用时甚至整体省略。原实现用 `#[serde(default)]` 把「省略」当 0，
    /// 于是第二次调用的 fragment 被并进 index=0，两次调用的 arguments 被 `push_str`
    /// 拼成 `{...}{...}`（非法 JSON）→ 参数整包退化为 `null`，报错还指不到病因。
    ///
    /// 这里按「id 优先」消歧：能对上已有槽位就复用；对不上且已有其它调用则另开槽位。
    fn resolve_tool_call_index(&mut self, tc: &DeltaToolCall) -> usize {
        if let Some(index) = tc.index {
            return index;
        }
        self.saw_index_less_tool_call = true;
        if let Some(id) = tc.id.as_deref().filter(|s| !s.is_empty()) {
            if let Some((&index, _)) = self.tool_calls.iter().find(|(_, e)| e.id == id) {
                return index;
            }
            if let Some((&last, _)) = self.tool_calls.iter().next_back() {
                return last.saturating_add(1);
            }
        }
        // 无 id（或首个调用）：按协议默认槽位 0
        0
    }
}

/// 单条 SSE 行的解析结果：`[DONE]` 终止标记或一个流式 chunk
enum SseEvent {
    Done,
    Chunk(StreamChunk),
}

/// 解析单条 SSE 行；非 `data:` 行（空行 / 注释 / event 字段）忽略
fn parse_sse_line(line: &str) -> Result<Option<SseEvent>> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(':') {
        return Ok(None);
    }
    let Some(data) = line.strip_prefix("data:") else {
        return Ok(None);
    };
    let data = data.trim();
    if data == "[DONE]" {
        return Ok(Some(SseEvent::Done));
    }
    serde_json::from_str::<StreamChunk>(data)
        .map(|chunk| Some(SseEvent::Chunk(chunk)))
        .map_err(|e| err!(Internal, "chat completions sse chunk parse failed: {e}"))
}

#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    usage: Option<Usage>,
}

#[derive(Deserialize, Default)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
    /// 结束原因（`stop` / `length` / `tool_calls` / `content_filter`…），仅在流末尾的 chunk 出现
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct StreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Deserialize)]
struct DeltaToolCall {
    /// 分组依据。**不能 `#[serde(default)]`**：省略会被当成 0，
    /// 并行调用的第二路 fragment 会被并进第一路（见 [`StreamAccumulator::resolve_tool_call_index`]）
    index: Option<usize>,
    id: Option<String>,
    function: Option<DeltaToolFunction>,
}

#[derive(Deserialize)]
struct DeltaToolFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f64>,
}

#[derive(Deserialize)]
struct MultimodalEmbeddingResponse {
    data: MultimodalEmbeddingData,
}

#[derive(Deserialize)]
struct MultimodalEmbeddingData {
    embedding: Vec<f64>,
}

// ==================== 非流式响应结构（方案 §四） ====================

/// 非流式 Chat Completions 响应（OpenAI 协议一次性 JSON）
#[derive(Debug, Deserialize)]
struct NonStreamResponse {
    /// 正常响应恒有 choices；缺失/为空按异常响应显式失败
    #[serde(default)]
    choices: Vec<NonStreamChoice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct NonStreamChoice {
    message: Option<NonStreamMessage>,
    /// 正常结束的证据（截断防护：缺失即显式失败，见 [`parse_non_stream_body`]）
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NonStreamMessage {
    content: Option<String>,
    tool_calls: Option<Vec<NonStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct NonStreamToolCall {
    id: Option<String>,
    function: Option<NonStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct NonStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[cfg(test)]
mod tests;
