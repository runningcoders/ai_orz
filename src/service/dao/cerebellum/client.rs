//! System One HTTP client（jev 协议编解码）
//!
//! wire format（官方公开口径，多来源交叉实证）：
//! - `POST {base}/v1/systemone`，`Authorization: Bearer <api_key>`
//! - 请求：`{"model": po.model_name, "state": <字符串|对象|数组>, "questions": {qid: {type, instructions, criteria?}}}`
//! - 响应：`{"model": "...", "answers": {qid: <类型化答案>}, "usage": {...}}`
//! - 结构校验：顶层缺 answers / answers 非对象 → 硬错误；
//!   单个答案解析失败 → 容错跳过（协议版本演进容忍，不拖垮整批结果）；
//!   config 解析失败 → 超时兜底默认值（与 cortex resolve_access_mode 同款容错口径）

use crate::models::cerebellum_types::{
    CerebellumAnswer, CerebellumQuestion, CerebellumRequest, CerebellumUsage, ThinkFastResult,
};
use crate::models::model_provider::{ModelProviderConfig, ModelProviderPo};
use crate::pkg::RequestContext;
use common::error::{Error, ErrorCode, Result, err};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

use super::{DEFAULT_CEREBELLUM_TIMEOUT_MS, DEFAULT_SYSTEM_ONE_BASE_URL};

/// 解析请求超时（毫秒）：config.timeout_ms 优先；缺省 / 脏 config 兜底默认值
fn resolve_timeout_ms(ctx: &RequestContext, provider: &ModelProviderPo) -> u64 {
    match serde_json::from_str::<ModelProviderConfig>(&provider.config) {
        Ok(cfg) => cfg.timeout_ms.unwrap_or(DEFAULT_CEREBELLUM_TIMEOUT_MS),
        Err(e) => {
            log_warn!(
                ctx,
                "cerebellum_think_fast",
                "provider config parse failed, falling back to default timeout: provider_id={} error={}",
                provider.id,
                e
            );
            DEFAULT_CEREBELLUM_TIMEOUT_MS
        }
    }
}

/// 校验 provider 出站前置条件 + 解析 System One 端点
///
/// `api_key` 为空硬错误（同 cortex validate_provider_for_request 口径：
/// 空 key 发出去只会被服务端拒绝）；`base_url` 非强依赖，缺省走
/// System One 默认端点常量兜底。
fn resolve_endpoint(provider: &ModelProviderPo) -> Result<String> {
    if provider.api_key.trim().is_empty() {
        return Err(err!(
            ConfigInvalid,
            "model provider api_key is empty, cannot call model API"
        ));
    }
    let base = provider
        .base_url
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SYSTEM_ONE_BASE_URL.to_string());
    Ok(format!("{}/v1/systemone", base.trim_end_matches('/')))
}

/// 快判断入口（协议编解码 + 响应结构校验）
pub(super) async fn think_fast(
    ctx: &RequestContext,
    client: &reqwest::Client,
    provider: &ModelProviderPo,
    state: Value,
    questions: BTreeMap<String, CerebellumQuestion>,
) -> Result<ThinkFastResult> {
    if questions.is_empty() {
        return Err(err!(
            ConfigInvalid,
            "cerebellum think_fast requires at least one question"
        ));
    }
    for (qid, q) in &questions {
        if let Err(e) = q.validate() {
            return Err(err!(
                ConfigInvalid,
                "cerebellum question '{}' invalid: {}",
                qid,
                e
            ));
        }
    }

    let url = resolve_endpoint(provider)?;
    let timeout_ms = resolve_timeout_ms(ctx, provider);
    let body = CerebellumRequest {
        model: provider.model_name.clone(),
        state,
        questions,
    };

    log_debug!(
        ctx,
        "cerebellum_think_fast",
        "POST {} model={} questions={}",
        url,
        body.model,
        body.questions.len()
    );

    let resp = client
        .post(&url)
        .bearer_auth(&provider.api_key)
        .timeout(Duration::from_millis(timeout_ms))
        .json(&body)
        .send()
        .await
        .map_err(|e| transport_error("system one", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp
            .text()
            .await
            .unwrap_or_else(|_| "<no body>".to_string());
        return Err(model_call_error("system one", status, &text));
    }

    // 结构校验：顶层必须携带对象形态的 answers（协议版本容错边界内的硬校验）
    let raw: Value = resp
        .json()
        .await
        .map_err(|e| transport_error("system one response", e))?;
    let answers_val = raw
        .get("answers")
        .ok_or_else(|| err!(Internal, "cerebellum response missing 'answers' field"))?
        .as_object()
        .ok_or_else(|| err!(Internal, "cerebellum response 'answers' is not an object"))?
        .clone();

    // 逐答案容错解析：未知/变形答案类型跳过并告警（协议版本演进容忍）
    let mut answers = BTreeMap::new();
    for (qid, v) in answers_val {
        match serde_json::from_value::<CerebellumAnswer>(v) {
            Ok(a) => {
                answers.insert(qid, a);
            }
            Err(e) => {
                log_warn!(
                    ctx,
                    "cerebellum_think_fast",
                    "skip unparseable answer (protocol tolerance): qid={} error={}",
                    qid,
                    e
                );
            }
        }
    }

    let usage: CerebellumUsage = match raw.get("usage").cloned() {
        Some(v) => serde_json::from_value::<CerebellumUsage>(v).unwrap_or_default(),
        None => CerebellumUsage::default(),
    };

    Ok(ThinkFastResult { answers, usage })
}

// ==================== 错误分类（与 cortex http.rs 同款口径） ====================

/// 传输阶段的错误分类与根因提取（保持 Internal 错误码，重试策略归上层裁决）
fn transport_error(kind: &str, e: reqwest::Error) -> Error {
    let tag = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else {
        "send"
    };
    let root = error_root_cause(&e);
    let root = if root.is_empty() {
        "<no cause>".to_string()
    } else {
        root
    };
    err!(
        Internal,
        "{kind} request failed ({tag}): {e}; root cause: {root}"
    )
}

/// 沿 error source 链走到最底层，取根因文本
fn error_root_cause(e: &reqwest::Error) -> String {
    let mut current: &dyn std::error::Error = e;
    while let Some(source) = current.source() {
        current = source;
    }
    current.to_string()
}

/// 将模型 HTTP 调用的非成功状态码映射为具体的模型错误码（与 cortex 口径一致）
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
        // 客户端错误：默认请求非法，响应体表明内容过滤则单独归类
        400..=499 => {
            if is_content_filtered(text) {
                Error::new(ErrorCode::ModelContentFiltered, detail)
            } else {
                Error::new(ErrorCode::ModelBadRequest, detail)
            }
        }
        // 服务端/网关错误：可重试
        500..=599 => Error::new(ErrorCode::ModelServerError, detail),
        _ => Error::new(ErrorCode::ModelServerError, detail),
    }
}

/// 粗略判断响应体是否为内容过滤类错误
fn is_content_filtered(text: &str) -> bool {
    text.contains("content_filter") || text.contains("moderation")
}

#[cfg(test)]
mod cerebellum_client_tests {
    use super::*;
    use common::enums::{ModelCapability, ProviderType};

    fn provider(config: &str) -> ModelProviderPo {
        let mut po = ModelProviderPo::new(
            "小脑-测试".to_string(),
            ProviderType::Jev,
            ModelCapability::Decision,
            "jev-latest".to_string(),
            "sk-test".to_string(),
            None,
            None,
            "test".to_string(),
        );
        po.config = config.to_string();
        po
    }

    #[test]
    fn endpoint_defaults_to_system_one_base() {
        let po = provider("{}");
        let url = resolve_endpoint(&po).unwrap();
        assert_eq!(url, "https://api.typesafe.ai/v1/systemone");
    }

    #[test]
    fn endpoint_prefers_provider_base_url() {
        let mut po = provider("{}");
        po.base_url = Some("https://my-relay.example.com/".to_string());
        let url = resolve_endpoint(&po).unwrap();
        assert_eq!(url, "https://my-relay.example.com/v1/systemone");
    }

    #[test]
    fn endpoint_rejects_empty_api_key() {
        let mut po = provider("{}");
        po.api_key = "  ".to_string();
        assert!(resolve_endpoint(&po).is_err());
    }

    #[test]
    fn timeout_uses_config_then_default() {
        let ctx = RequestContext::new_system();
        assert_eq!(
            resolve_timeout_ms(&ctx, &provider("{}")),
            DEFAULT_CEREBELLUM_TIMEOUT_MS
        );
        assert_eq!(
            resolve_timeout_ms(&ctx, &provider(r#"{"timeout_ms": 500}"#)),
            500
        );
        // 脏 config 容错：解析失败兜底默认值
        assert_eq!(
            resolve_timeout_ms(&ctx, &provider("not-json")),
            DEFAULT_CEREBELLUM_TIMEOUT_MS
        );
    }
}
