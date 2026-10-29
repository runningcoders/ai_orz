//! OpenAI Responses API（`/v1/responses`）CortexDao 实现
//!
//! 与 Chat Completions 的差异**不在传输形态**（两者都支持 stream / non_stream，
//! 由 `config.access_mode` 独立控制），而在 wire 协议本身：
//! - 请求：`messages` → `input` **item 数组**；`tools` 由嵌套 `{function:{...}}`
//!   变为**扁平** `{type,name,description,parameters,strict}`。
//! - 响应：不再是 `choices[0].delta`，而是**语义事件流**
//!   （`response.output_text.delta` / `response.function_call_arguments.delta` /
//!   `response.completed` …）。
//! - 终态：没有 `finish_reason`，改由顶层 `status` + `incomplete_details.reason` 表达。
//!
//! ## 关键设计：不逐 delta 累加，改读终态 envelope
//!
//! `CortexDao::think()` **不是增量接口**——它收完整轮结果后一次性返回 `ThinkResult`，
//! 流式在这里只用于空闲超时检测（长生成不被总时长误杀），并不向调用方渐进吐字。
//! 因此本实现**只消费终态事件**（`response.completed` / `response.incomplete` /
//! `response.failed` / `error`），再解析其 `response.output` 数组。收益有三：
//! 1. 流式与非流式**共用同一条解析路径**，消除两份实现逐渐分歧的风险；
//! 2. 绕开 delta 拼接状态机——「工具参数被拼成半截 JSON → 整包退化 null」那类
//!    静默 bug 在本协议下不复存在；
//! 3. 终态缺失即显式失败，绝不返回半截结果。
//!
//! 代价：失去未来做渐进式 UI 的可能。届时再加 delta 聚合，不影响本文件的对外契约。

use crate::models::cortex_types::{ChatMessage, ReasoningItem, ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::pkg::http::presets;
use crate::service::dao::cortex::native::http::{
    AccumToolCall, CompletionTokensDetails, PromptTokensDetails, StreamAccumulator, Usage,
    default_base_url, finish_think_result, model_call_error, resolve_access_mode, resolve_base_url,
    resolve_intensity, transport_error, validate_provider_for_request,
};
use async_trait::async_trait;
use common::enums::ModelAccessMode;
use common::error::{Error, ErrorCode, Result, err};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

/// Responses API CortexDao 实现
///
/// 无状态单例：所有配置从 `&ModelProviderPo` 读取，自身仅持有共享的 reqwest::Client。
pub struct ResponsesCortexDao {
    client: reqwest::Client,
}

impl ResponsesCortexDao {
    pub fn new() -> Self {
        let client = presets::llm()
            .build()
            .expect("Failed to build reqwest client for ResponsesCortexDao");
        Self { client }
    }
}

impl Default for ResponsesCortexDao {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::CortexDao for ResponsesCortexDao {
    async fn think(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        messages: &[ChatMessage],
        tools: &[ToolDescriptor],
        thinking_intensity: Option<common::enums::ThinkingIntensity>,
    ) -> Result<ThinkResult> {
        call_responses(
            ctx,
            &self.client,
            provider,
            messages,
            tools,
            thinking_intensity,
        )
        .await
    }
}

/// 调用 Responses API 并组装 [`ThinkResult`]
pub async fn call_responses(
    ctx: RequestContext,
    client: &reqwest::Client,
    provider: &ModelProviderPo,
    messages: &[ChatMessage],
    tools: &[ToolDescriptor],
    thinking_intensity: Option<common::enums::ThinkingIntensity>,
) -> Result<ThinkResult> {
    validate_provider_for_request(provider)?;
    let base_url = resolve_base_url(provider, default_base_url(provider.provider_type));
    let url = format!("{}/responses", base_url.trim_end_matches('/'));

    // 与 chat 侧同源：缺省/脏 config 兜底 stream，不中断推理
    let access_mode = resolve_access_mode(&ctx, provider);
    let body = build_responses_body(provider, messages, tools, access_mode, thinking_intensity);

    log_debug!(
        ctx.clone(),
        "cortex_responses_request",
        "POST {} model={} messages={} tools={}",
        url,
        provider.model_name,
        messages.len(),
        tools.len()
    );

    let resp = client
        .post(&url)
        .bearer_auth(&provider.api_key)
        .timeout(presets::LLM_STREAM_TOTAL_TIMEOUT)
        .json(&body)
        .send()
        .await
        .map_err(|e| transport_error("responses", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp
            .text()
            .await
            .unwrap_or_else(|_| "<no body>".to_string());
        return Err(model_call_error("responses", status, &text));
    }

    let response_obj = match access_mode {
        ModelAccessMode::Stream => {
            consume_responses_stream(resp.bytes_stream(), presets::LLM_STREAM_IDLE_TIMEOUT).await?
        }
        ModelAccessMode::NonStream => {
            let text = resp
                .text()
                .await
                .map_err(|e| transport_error("responses body", e))?;
            serde_json::from_str::<ResponseObject>(&text)
                .map_err(|e| err!(Internal, "responses body parse failed: {e}"))?
        }
    };

    parse_response_object(&ctx, response_obj)
}

/// 构建 Responses API 请求体
fn build_responses_body(
    provider: &ModelProviderPo,
    messages: &[ChatMessage],
    tools: &[ToolDescriptor],
    access_mode: ModelAccessMode,
    thinking_intensity: Option<common::enums::ThinkingIntensity>,
) -> Value {
    let mut body = json!({
        "model": provider.model_name,
        "input": messages_to_input(messages),
        // 显式关闭服务端留存：默认会留存请求 payload，与平台脱敏的
        // 「主路径受控 / 不可逆旁路需豁免」分层原则冲突。
        //
        // 代价：`store=false` 时部分实现不再返回 reasoning 的 `encrypted_content`，
        // 回传退化为仅带 id + summary（见 [`parse_response_object`]）。
        "store": false,
    });
    // 思考强度：Provider 未配置视为能力开关关闭，整条链路不下发；
    // 配置后以调用方 override 优先，否则回退 provider 基准分数。
    // 平台 0–100 分在此按下游三档（low/medium/high）转换为 wire 值。
    if let Some(intensity) = resolve_intensity(provider, thinking_intensity) {
        body["reasoning"] = json!({ "effort": intensity.to_reasoning_effort_wire() });
    }
    if access_mode == ModelAccessMode::Stream {
        body["stream"] = json!(true);
    }
    if !tools.is_empty() {
        body["tools"] = json!(
            tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                        // 第一版恒 false：MCP 同步进来的 inputSchema 是不可信输入，
                        // strict 校验会挑刺并 400；关掉只是少一层校验，不影响调用。
                        "strict": false,
                    })
                })
                .collect::<Vec<_>>()
        );
    }
    body
}

/// `ChatMessage` 数组 → Responses 的 `input` item 数组
///
/// 与 chat 协议的 `messages_to_json` 是**一对平级实现**，不是它的变体：
/// 这里一条 `Assistant` 会展开成 1 个 message item + N 个 function_call item。
fn messages_to_input(messages: &[ChatMessage]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(messages.len());
    for m in messages {
        match m {
            // 系统消息留在 input 内而**不用顶层 `instructions`**：系统消息可能有多条
            // （人设 / 技能方法论 / 核心规则），合并会丢结构。
            ChatMessage::System { content } => out.push(json!({
                "type": "message",
                "role": "system",
                "content": [{"type": "input_text", "text": content}],
            })),
            ChatMessage::User { content } => out.push(json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": content}],
            })),
            ChatMessage::UserMultimodal { text, images } => {
                let mut parts = vec![json!({"type": "input_text", "text": text})];
                for img in images {
                    parts.push(json!({
                        "type": "input_image",
                        "image_url": format!("data:{};base64,{}", img.mime_type, img.data_base64),
                    }));
                }
                out.push(json!({"type": "message", "role": "user", "content": parts}));
            }
            // 一条变多条：Responses 的 input 是**扁平 item 流**，没有嵌套 tool_calls 字段
            ChatMessage::Assistant {
                content,
                tool_calls,
            } => {
                // 空 content 不产出 message item（空 content 数组会被部分实现拒收）
                if let Some(c) = content.as_ref().filter(|c| !c.is_empty()) {
                    out.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": c}],
                    }));
                }
                for tc in tool_calls.iter().flatten() {
                    out.push(json!({
                        "type": "function_call",
                        "call_id": tc.id,
                        "name": tc.name,
                        "arguments": tc.arguments.to_string(),
                    }));
                }
            }
            ChatMessage::Tool {
                tool_call_id,
                content,
            } => out.push(json!({
                "type": "function_call_output",
                "call_id": tool_call_id,
                "output": content,
            })),
            // 上一轮的推理项原样回传（部分推理模型强制要求，否则 400）
            ChatMessage::Reasoning { item } => out.push(reasoning_item_to_json(item)),
        }
    }
    out
}

/// reasoning item 序列化为 Responses 的 input item
///
/// ⚠️ `summary` **恒输出**（空时为 `[]`），不得因空而省略 key：部分网关
/// （modelhub 实测）会把「字段缺失」规范化成 `summary: null` 再校验，
/// 报 400 `Invalid type for 'input[N].summary': expected an array of
/// objects, but got null instead`。触发场景：`store=false` 时无
/// `encrypted_content`、模型本轮未吐 summary 文本 ⇒ 上轮 reasoning item
/// 回传时 summary 为空数组，是最常见的常态路径。
fn reasoning_item_to_json(item: &ReasoningItem) -> Value {
    let mut v = json!({"type": "reasoning"});
    if let Some(id) = &item.id {
        v["id"] = json!(id);
    }
    v["summary"] = json!(
        item.summary
            .iter()
            .map(|s| json!({"type": "summary_text", "text": s}))
            .collect::<Vec<_>>()
    );
    if let Some(enc) = &item.encrypted_content {
        v["encrypted_content"] = json!(enc);
    }
    v
}

// ==================== 响应结构 ====================

/// Responses 响应对象（非流式响应体，或终态事件里的 `response` 字段）
#[derive(Debug, Deserialize)]
struct ResponseObject {
    /// `completed` / `incomplete` / `failed` / `cancelled`
    status: Option<String>,
    #[serde(default)]
    output: Vec<OutputItem>,
    usage: Option<ResponsesUsage>,
    incomplete_details: Option<IncompleteDetails>,
    error: Option<ResponsesError>,
}

/// output item：用**宽松结构**而非 `#[serde(tag = "type")]` 枚举。
///
/// Responses 可能输出我们未声明的 item 类型（web_search_call / file_search_call /
/// code_interpreter_call / image_generation_call …）。tagged 枚举遇到未知 type 会
/// 直接反序列化失败，把「模型用了个内置工具」升级成「整轮解析失败」。这里全部字段
/// 取 Option，未知 item 自然落空被忽略。
#[derive(Debug, Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    kind: Option<String>,
    /// message：content part 数组
    content: Option<Vec<ContentPart>>,
    /// function_call
    call_id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
    /// reasoning
    id: Option<String>,
    summary: Option<Vec<SummaryPart>>,
    encrypted_content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContentPart {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SummaryPart {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponsesUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    input_tokens_details: Option<InputTokensDetails>,
    output_tokens_details: Option<OutputTokensDetails>,
}

#[derive(Debug, Deserialize)]
struct InputTokensDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OutputTokensDetails {
    reasoning_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct IncompleteDetails {
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponsesError {
    code: Option<String>,
    message: Option<String>,
}

/// 把终态响应对象组装为 [`ThinkResult`]
///
/// 两条访问模式（流式 / 非流式）在此汇合，保证只有一份终态判定与工具解析实现。
fn parse_response_object(ctx: &RequestContext, obj: ResponseObject) -> Result<ThinkResult> {
    // 终态判定：Responses **没有 finish_reason**，改由 status 承担。
    // 与 chat 侧同源口径——截断/过滤一律显式失败，绝不静默返回半截结果。
    match obj.status.as_deref() {
        Some("completed") => {}
        Some("incomplete") => {
            let reason = obj
                .incomplete_details
                .as_ref()
                .and_then(|d| d.reason.clone())
                .unwrap_or_else(|| "unknown".to_string());
            return Err(err!(
                Internal,
                "responses incomplete (output truncated): reason={reason} output_items={}",
                obj.output.len()
            ));
        }
        Some("failed") => {
            let (code, message) = error_fields(obj.error.as_ref());
            return Err(responses_error("responses failed", &code, &message));
        }
        other => {
            return Err(err!(
                Internal,
                "responses missing or unknown status: `{}` (expected completed/incomplete/failed)",
                other.unwrap_or("<none>")
            ));
        }
    }

    let mut content = String::new();
    let mut saw_content = false;
    let mut tool_calls: BTreeMap<usize, AccumToolCall> = BTreeMap::new();
    let mut reasoning_items: Vec<ReasoningItem> = Vec::new();

    for item in &obj.output {
        match item.kind.as_deref() {
            Some("message") => {
                for part in item.content.iter().flatten() {
                    if part.kind.as_deref() != Some("output_text") {
                        continue;
                    }
                    if let Some(t) = &part.text {
                        saw_content = true;
                        content.push_str(t);
                    }
                }
            }
            // **没有 finish_reason 可用**：chat 侧靠标志位判断「本轮是否要调工具」，
            // Responses 只能扫描 output 数组里有没有 function_call item。
            // 这里一旦漏判，表现是「模型要求调工具但被当成 Final 直接返回」——
            // 不报错、工具不执行，属静默退化，故单独强调。
            Some("function_call") => {
                let idx = tool_calls.len();
                tool_calls.insert(
                    idx,
                    AccumToolCall {
                        id: item.call_id.clone().unwrap_or_default(),
                        name: item.name.clone().unwrap_or_default(),
                        arguments: item.arguments.clone().unwrap_or_default(),
                    },
                );
            }
            Some("reasoning") => {
                reasoning_items.push(ReasoningItem {
                    id: item.id.clone(),
                    summary: item
                        .summary
                        .iter()
                        .flatten()
                        .filter_map(|s| s.text.clone())
                        .collect(),
                    encrypted_content: item.encrypted_content.clone(),
                });
            }
            _ => {}
        }
    }

    // 复用 chat 侧的聚合结构与组装函数：两条协议的「有 tool_calls → ToolCall
    // 否则 Final」判定、参数解析与失败留痕只有一份实现。
    let acc = StreamAccumulator {
        content,
        saw_content,
        saw_done: true,
        finish_reason: None,
        saw_index_less_tool_call: false,
        tool_calls,
        usage: obj.usage.as_ref().map(responses_usage_to_usage),
    };
    Ok(finish_think_result(ctx, acc, reasoning_items))
}

fn error_fields(e: Option<&ResponsesError>) -> (String, String) {
    match e {
        Some(e) => (
            e.code.clone().unwrap_or_else(|| "-".to_string()),
            e.message.clone().unwrap_or_else(|| "-".to_string()),
        ),
        None => ("-".to_string(), "-".to_string()),
    }
}

/// 把 Responses 协议层的错误信息分类为具体错误码
///
/// 覆盖三条路径：流内 `error` 事件、流内 `response.failed`、非流式
/// `status:"failed"`（三者此前一律 `err!(Internal, ...)`）。
///
/// ⚠️ 与 HTTP 层 [`model_call_error`] 同口径：**限流必须落到 `ModelRateLimited`**。
/// 退化为 `Internal` 的后果远不止文案不对——`is_model_error()` 不认 `Internal`，
/// AOP 消费者会按普通故障 nack 重投（见 `src/consumer/message.rs`），在上游限流
/// 期间反复重试等于火上浇油，且不会通知用户；而 HTTP 429 路径却是「不重试 +
/// 告知用户」。同一故障两种行为，正是此处要消除的分歧。
fn responses_error(kind: &str, code: &str, message: &str) -> Error {
    if is_rate_limited(code, message) {
        return Error::new(
            ErrorCode::ModelRateLimited,
            format!("{kind} rate limited: code={code} message={message}"),
        );
    }
    err!(Internal, "{kind}: code={code} message={message}")
}

/// 判断 Responses 错误是否为限流
///
/// `code` 是上游**机器生成的枚举**（非模型自由文本），优先按封闭词表全等匹配；
/// 仅在上游不返回 code（此处缺省为 `-`）时才回退到 message 特征串（小写包含）。
fn is_rate_limited(code: &str, message: &str) -> bool {
    const RATE_LIMIT_CODES: [&str; 5] = [
        "rate_limit",
        "rate_limit_exceeded",
        "rate_limited",
        "rate-limit",
        "too_many_requests",
    ];
    let code = code.trim().to_ascii_lowercase();
    if RATE_LIMIT_CODES.contains(&code.as_str()) {
        return true;
    }
    let msg = message.to_ascii_lowercase();
    msg.contains("rate limit") || msg.contains("rate_limit") || msg.contains("too many requests")
}

/// Responses 的 usage 折算为内部 [`Usage`]（字段命名对齐 Chat Completions），
/// 从而共用 `finish_think_result` 里那条 `TokenUsage` 归并路径。
fn responses_usage_to_usage(u: &ResponsesUsage) -> Usage {
    Usage {
        prompt_tokens: u.input_tokens,
        completion_tokens: u.output_tokens,
        total_tokens: u.total_tokens,
        prompt_tokens_details: u
            .input_tokens_details
            .as_ref()
            .map(|d| PromptTokensDetails {
                cached_tokens: d.cached_tokens,
            }),
        completion_tokens_details: u.output_tokens_details.as_ref().map(|d| {
            CompletionTokensDetails {
                reasoning_tokens: d.reasoning_tokens,
            }
        }),
    }
}

// ==================== 流式事件消费 ====================

/// 单条 SSE 行的解析结果
enum ResponsesSse {
    /// 非终态事件（created / in_progress / 各种 delta / output_item.*）——忽略
    Ignore,
    /// 终态事件，携带响应对象
    Terminal(ResponseObject),
    /// 失败终态，携带**已分类**的错误（限流 → `ModelRateLimited`，否则 `Internal`）
    ///
    /// 直接带 [`Error`] 而非格式化字符串：分类逻辑收口在 [`responses_error`]，
    /// 消费侧无条件 `Err(e)`，杜绝「同一故障在不同路径得到不同错误码」。
    Failed(Error),
}

/// 消费 Responses 事件流，**只取终态 envelope**（见模块级注释）
async fn consume_responses_stream<S, B>(stream: S, idle_timeout: Duration) -> Result<ResponseObject>
where
    S: Stream<Item = reqwest::Result<B>>,
    B: AsRef<[u8]>,
{
    let mut stream = Box::pin(stream);
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let chunk = match tokio::time::timeout(idle_timeout, stream.next()).await {
            Ok(None) => break,
            Err(_) => {
                return Err(err!(
                    Internal,
                    "responses stream idle timeout: no data for {idle_timeout:?}"
                ));
            }
            Ok(Some(item)) => item.map_err(|e| transport_error("responses stream", e))?,
        };
        buf.extend_from_slice(chunk.as_ref());
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            match parse_responses_sse_line(&line)? {
                ResponsesSse::Ignore => {}
                ResponsesSse::Terminal(obj) => return Ok(obj),
                ResponsesSse::Failed(e) => return Err(e),
            }
        }
    }
    // 容忍服务端未以换行收尾的最后一行
    if !buf.is_empty() {
        let line = String::from_utf8_lossy(&buf);
        match parse_responses_sse_line(&line)? {
            ResponsesSse::Terminal(obj) => return Ok(obj),
            ResponsesSse::Failed(e) => return Err(e),
            ResponsesSse::Ignore => {}
        }
    }
    // 走到这里说明连接在中途被切断且**没有任何终态事件**。
    // 显式失败：半截结果一旦当成功返回，下游只会看到「工具参数整包为 null」
    // 这种与病因完全无关的错误。
    Err(err!(
        Internal,
        "responses stream ended without a terminal event \
         (completed/incomplete/failed): connection cut mid-stream"
    ))
}

fn parse_responses_sse_line(line: &str) -> Result<ResponsesSse> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(':') {
        return Ok(ResponsesSse::Ignore);
    }
    let Some(data) = line.strip_prefix("data:") else {
        return Ok(ResponsesSse::Ignore);
    };
    let data = data.trim();
    if data == "[DONE]" {
        return Ok(ResponsesSse::Ignore);
    }
    let value: Value = serde_json::from_str(data)
        .map_err(|e| err!(Internal, "responses sse event parse failed: {e}"))?;
    let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match kind {
        // 终态：payload 里的 `response` 含完整 output 数组 + usage
        "response.completed" | "response.incomplete" => {
            Ok(ResponsesSse::Terminal(terminal_object(&value, kind)?))
        }
        "response.failed" => {
            let obj = terminal_object(&value, kind)?;
            let (code, message) = error_fields(obj.error.as_ref());
            Ok(ResponsesSse::Failed(responses_error(
                "responses stream failed",
                &code,
                &message,
            )))
        }
        // 带外错误事件（`response.failed` 是生成失败，`error` 是传输/流层错误）
        "error" => {
            let code = value
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("-")
                .to_string();
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("-")
                .to_string();
            Ok(ResponsesSse::Failed(responses_error(
                "responses error event",
                &code,
                &message,
            )))
        }
        // 其余事件一律忽略：终态 envelope 已含完整 output，delta 在本实现里无消费者。
        _ => Ok(ResponsesSse::Ignore),
    }
}

/// 从终态事件里取出 `response` 子对象
fn terminal_object(value: &Value, kind: &str) -> Result<ResponseObject> {
    serde_json::from_value(value.get("response").cloned().unwrap_or(Value::Null))
        .map_err(|e| err!(Internal, "responses `{kind}` payload parse failed: {e}"))
}

#[cfg(test)]
mod tests;
