//! HTTP 辅助函数 - 直接调用 OpenAI 兼容 API
//!
//! 所有 provider 统一走 OpenAI Chat Completions / Embeddings 协议，
//! 不再依赖 rig 的 client/agent 抽象。

use crate::models::cortex_types::{
    ChatMessage, ThinkResult, TokenUsage, ToolCallRequest, ToolDescriptor,
};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use common::error::{Result, err};
use serde::Deserialize;
use serde_json::{Value, json};

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

/// 将 ChatMessage 数组转换为 OpenAI API 格式的 JSON 数组
fn messages_to_json(messages: &[ChatMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| match m {
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
/// 所有 provider 统一走 /chat/completions endpoint
pub async fn call_chat_completions(
    ctx: RequestContext,
    client: &reqwest::Client,
    provider: &ModelProviderPo,
    messages: &[ChatMessage],
    tools: &[ToolDescriptor],
) -> Result<ThinkResult> {
    let base_url = resolve_base_url(provider, default_base_url(provider.provider_type));
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    // 构建请求体
    let mut body = json!({
        "model": provider.model_name,
        "messages": messages_to_json(messages),
        "stream": false,
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
        .json(&body)
        .send()
        .await
        .map_err(|e| err!(Internal, "chat completions request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp
            .text()
            .await
            .unwrap_or_else(|_| "<no body>".to_string());
        return Err(err!(
            Internal,
            "chat completions failed ({}): {}",
            status,
            text
        ));
    }

    let resp_body: ChatCompletionResponse = resp
        .json()
        .await
        .map_err(|e| err!(Internal, "chat completions response parse failed: {}", e))?;

    // 提取 token usage
    let usage = TokenUsage {
        input_tokens: resp_body.usage.prompt_tokens.unwrap_or(0),
        output_tokens: resp_body.usage.completion_tokens.unwrap_or(0),
        total_tokens: resp_body.usage.total_tokens,
    };

    // 解析 choices
    let choice = resp_body
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| err!(Internal, "chat completions: no choices in response"))?;

    let message = choice.message;

    // 判断是否有 tool_calls
    if let Some(tool_calls) = message.tool_calls
        && !tool_calls.is_empty()
    {
        let calls: Vec<ToolCallRequest> = tool_calls
            .into_iter()
            .map(|tc| {
                let arguments: Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
                ToolCallRequest {
                    id: tc.id,
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect();

        return Ok(ThinkResult::ToolCall {
            content: message.content,
            tool_calls: calls,
            usage,
        });
    }

    // 最终回答
    Ok(ThinkResult::Final {
        content: message.content.unwrap_or_default(),
        usage,
    })
}

/// 调用标准 Embeddings API
pub async fn call_embeddings(
    client: &reqwest::Client,
    provider: &ModelProviderPo,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
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
        .map_err(|e| err!(Internal, "embeddings request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp
            .text()
            .await
            .unwrap_or_else(|_| "<no body>".to_string());
        return Err(err!(Internal, "embeddings failed ({}): {}", status, text));
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
            .map_err(|e| err!(Internal, "DoubaoVision embedding request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "<no body>".to_string());
            return Err(err!(
                Internal,
                "DoubaoVision embedding failed ({}): {}",
                status,
                text
            ));
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

// ==================== 请求/响应结构 ====================

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    usage: Usage,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Deserialize)]
struct ToolCall {
    id: String,
    function: ToolCallFunction,
}

#[derive(Deserialize)]
struct ToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
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
