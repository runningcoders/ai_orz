//! HTTP 辅助函数 - 直接调用 OpenAI 兼容 API
//!
//! 所有 provider 统一走 OpenAI Chat Completions / Embeddings 协议，
//! 不再依赖 rig 的 client/agent 抽象。

use crate::models::cortex_types::{
    ChatMessage, ThinkResult, TokenUsage, ToolCallRequest, ToolDescriptor,
};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::pkg::http::presets;
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

/// 调用 Chat Completions API（流式）
///
/// 所有 provider 统一走 /chat/completions endpoint。请求侧启用 SSE 流式，
/// 超时判定从「请求总时长」改为「chunk 间隔空闲检测 + 总时长硬上限兜底」，
/// 流式聚合细节见 [`consume_think_stream`]。
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

    // 构建请求体：启用 SSE 流式；include_usage 让 usage 随最后一个 chunk 下发
    let mut body = json!({
        "model": provider.model_name,
        "messages": messages_to_json(messages),
        "stream": true,
        "stream_options": { "include_usage": true },
    });

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

    let acc = consume_think_stream(resp.bytes_stream(), presets::LLM_STREAM_IDLE_TIMEOUT).await?;
    Ok(finish_think_result(acc))
}

/// 将流式聚合结果组装为上层 [`ThinkResult`]（与原非流式解析约定一致）。
fn finish_think_result(acc: StreamAccumulator) -> ThinkResult {
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

    if !acc.tool_calls.is_empty() {
        let calls: Vec<ToolCallRequest> = acc
            .tool_calls
            .into_values()
            .map(|tc| ToolCallRequest {
                arguments: serde_json::from_str(&tc.arguments).unwrap_or(Value::Null),
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

/// 消费 SSE 响应流并聚合为 [`StreamAccumulator`]。
///
/// 超时判定（替代原「请求总时长」）：
/// - **空闲超时**：相邻 chunk 的最大间隔超过 `idle_timeout`（tokio 层逐 next 计时），
///   判定流中断返回错误。只要 token 持续产出即视为正常，长生成不再被总时长误杀。
/// - **总时长硬上限**：由请求构建处的逐请求 `.timeout()` 兜底（见调用方），
///   防服务端异常（如无限心跳）导致空闲判定永不触发。
async fn consume_think_stream<S, B>(stream: S, idle_timeout: Duration) -> Result<StreamAccumulator>
where
    S: Stream<Item = reqwest::Result<B>>,
    B: AsRef<[u8]>,
{
    let mut stream = Box::pin(stream);
    let mut acc = StreamAccumulator::default();
    let mut buf: Vec<u8> = Vec::new();
    loop {
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
                    SseEvent::Done => return Ok(acc),
                    SseEvent::Chunk(c) => acc.absorb(c),
                }
            }
        }
    }
    // 容忍服务端未以换行收尾的最后一行
    if !buf.is_empty() {
        let line = String::from_utf8_lossy(&buf);
        if let Some(event) = parse_sse_line(&line)? {
            match event {
                SseEvent::Done => {}
                SseEvent::Chunk(c) => acc.absorb(c),
            }
        }
    }
    Ok(acc)
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
            if let Some(c) = choice.delta.content {
                self.saw_content = true;
                self.content.push_str(&c);
            }
            for tc in choice.delta.tool_calls.into_iter().flatten() {
                let entry = self.tool_calls.entry(tc.index).or_default();
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
}

#[derive(Deserialize, Default)]
struct StreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Deserialize)]
struct DeltaToolCall {
    #[serde(default)]
    index: usize,
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

#[cfg(test)]
mod tests;
