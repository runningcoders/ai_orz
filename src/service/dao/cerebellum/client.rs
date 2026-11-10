//! System One HTTP client（jev 协议编解码）
//!
//! wire format（官方公开口径，多来源交叉实证）：
//! - `POST {base}/systemone`（`base` 自带版本前缀，如 `/v1`），`Authorization: Bearer <api_key>`
//! - 请求：`{"model": po.model_name, "state": <字符串|对象|数组>, "questions": {qid: {type, instructions, criteria?}}}`
//! - 响应：`{"model": "...", "answers": {qid: <类型化答案>}, "usage": {...}}`
//! - 结构校验：顶层缺 answers / answers 非对象 → 硬错误；
//!   单个答案解析失败 → 容错跳过（协议版本演进容忍，不拖垮整批结果）；
//!   config 解析失败 → 超时兜底默认值（与 cortex resolve_access_mode 同款容错口径）

use crate::models::model_provider::{ModelProviderConfig, ModelProviderPo};
use crate::pkg::RequestContext;
use common::api::cerebellum::{
    CerebellumAnswer, CerebellumQuestion, CerebellumRequest, CerebellumUsage, ThinkFastResult,
};
use common::error::{Error, ErrorCode, Result, err};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

use super::{DEFAULT_CEREBELLUM_TIMEOUT_MS, DEFAULT_SYSTEM_ONE_BASE_URL};

/// provider 配置里的**显式**超时（毫秒）；未配置 / 脏 config 一律 `None`
///
/// 单点解析：`resolve_timeout_ms`（本 client 的实际等待时长）与
/// `brain::effective_timeout_ms`（取值梯子的纯函数形态）都从这里取，
/// 避免两处各写一遍 `serde_json::from_str` 而出现「一处认 `timeout_ms`、
/// 另一处不认」的双源。
pub(crate) fn provider_timeout_ms(ctx: &RequestContext, provider: &ModelProviderPo) -> Option<u64> {
    match serde_json::from_str::<ModelProviderConfig>(&provider.config) {
        Ok(cfg) => cfg.timeout_ms,
        Err(e) => {
            log_warn!(
                ctx,
                "cerebellum_think_fast",
                "provider config parse failed, falling back to default timeout: provider_id={} error={}",
                provider.id,
                e
            );
            None
        }
    }
}

/// 解析请求超时（毫秒）：config.timeout_ms 优先；缺省 / 脏 config 兜底默认值
pub(crate) fn resolve_timeout_ms(ctx: &RequestContext, provider: &ModelProviderPo) -> u64 {
    provider_timeout_ms(ctx, provider).unwrap_or(DEFAULT_CEREBELLUM_TIMEOUT_MS)
}

/// 校验 provider 出站前置条件 + 解析 System One 端点
///
/// `api_key` 为空硬错误（同 cortex validate_provider_for_request 口径：
/// 空 key 发出去只会被服务端拒绝）；`base_url` 缺省值**按 provider_type 分派**
/// （见 [`default_base_url_for`]），不是全局一个常量。
fn resolve_endpoint(provider: &ModelProviderPo) -> Result<String> {
    if provider.api_key.trim().is_empty() {
        return Err(err!(
            ConfigInvalid,
            "model provider api_key is empty, cannot call model API"
        ));
    }
    let base = match provider.base_url.clone().filter(|s| !s.trim().is_empty()) {
        Some(b) => b,
        None => default_base_url_for(provider.provider_type)?.to_string(),
    };
    Ok(format!("{}/systemone", base.trim_end_matches('/')))
}

/// 缺省 System One endpoint（**按 provider_type 分派**）
///
/// - `Jev`：有平台内建的商业端点兜底 [`DEFAULT_SYSTEM_ONE_BASE_URL`]，存量行为零变化；
/// - 其余类型（含 `Qwen` = 阿里百炼）：**没有**可推断的缺省值。百炼决策模型走
///   **Workspace 专用 Endpoint**（`https://{WorkspaceId}.{region}.maas.aliyuncs.com/compatible-mode/v1`），
///   workspace id 与地域都编码在 host 里，平台无法凭空补全。
///
/// 为什么必须显式 `Err` 而不是统一兜底到 `DEFAULT_SYSTEM_ONE_BASE_URL`：
/// 统一兜底会把「百炼没配 base_url」静默改写成「请求打到 TypeSafe 商业端点」——
/// 失败信息与真实原因无关，排查会被带到完全错误的方向（同 R50 假绿的反面：
/// 这里要的是**红得准**）。
///
/// 降级不变量不变：本函数只返回 `Err`，上游归 `DegradedReason::CallError` /
/// `NoProvider` 走原逻辑，不 panic、不阻断主流程。
fn default_base_url_for(provider_type: common::enums::ProviderType) -> Result<&'static str> {
    match provider_type {
        common::enums::ProviderType::Jev => Ok(DEFAULT_SYSTEM_ONE_BASE_URL),
        other => Err(err!(
            ConfigInvalid,
            "system one provider type '{}' has no built-in default base_url; configure it explicitly \
             (Aliyun Bailian decision model must use its Workspace endpoint: \
             https://{{WorkspaceId}}.{{region}}.maas.aliyuncs.com/compatible-mode/v1)",
            other
        )),
    }
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
        po.base_url = Some("https://my-relay.example.com/v1/".to_string());
        let url = resolve_endpoint(&po).unwrap();
        assert_eq!(url, "https://my-relay.example.com/v1/systemone");
    }

    #[test]
    fn endpoint_rejects_empty_api_key() {
        let mut po = provider("{}");
        po.api_key = "  ".to_string();
        assert!(resolve_endpoint(&po).is_err());
    }

    /// 字面断言：缺省超时必须是 **3000ms**（AMan 2026-11-10 续令「缺省耗时改成 3 秒」，
    /// 消息 `01a1c5c9`；同日更早的「统一 5 秒」已作废）。
    ///
    /// 上一条用例引用常量，改常量时会「自动跟变」——它只能证明「配置优先、
    /// 缺省兜底」这条链路成立，**证不了缺省值到底是几百毫秒**。这里用字面
    /// 数字钉死：有人把它压回 1000 / 抬到 5000 或改成别的值，本用例立刻红。
    ///
    /// 两个方向都要钉：抬到 5000 已被同轮续令明确否决（「改成 3 秒吧」）；压回
    /// 1000 等于抗一条未被撤销的用户指令（`ea06affb` 那次误落 800 正是这么来的，
    /// 直到下一轮才被差值字面断言抓出来）。与场景覆盖的差值另有一条字面断言
    /// （`chain_reply_tests::scene_declares_a_5s_timeout_override`）。
    #[test]
    fn default_timeout_ms_is_3000_by_decision() {
        assert_eq!(
            DEFAULT_CEREBELLUM_TIMEOUT_MS, 3_000,
            "缺省小脑超时必须保持 3000ms：AMan 续令「缺省耗时改成 3 秒」，原「统一 5 秒」已作废"
        );
    }

    #[tokio::test]
    async fn timeout_uses_config_then_default() {
        // RequestContext::new_system() 依赖全局 Storage 单例（builder 未显式
        // 注入 storage 时回退全局），故先初始化——同其它 DAO 测试的约定。
        crate::pkg::storage::test_support::init_for_test().await;
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

    /// 百炼（Qwen）决策 provider 未配 base_url ⇒ **显式失败**
    ///
    /// 反事实对照（R49 口径，无需跑改造前代码即可判定）：本用例在改造前必红——
    /// 改造前 `resolve_endpoint` 对任何 provider_type 都兜底
    /// `https://api.typesafe.ai/v1`，`unwrap_err()` 直接 panic 在 `Ok` 上。
    ///
    /// 为什么这不是「故意找茬」：百炼决策模型没有可推断的缺省端点（workspace id
    /// 与地域都在 host 里），静默落到 TypeSafe 商业端点会让 401/DNS 错误掩盖
    /// 真实原因「你没填 base_url」。
    #[test]
    fn qwen_decision_provider_requires_explicit_base_url() {
        let mut po = provider("{}");
        po.provider_type = ProviderType::Qwen;
        po.base_url = None;
        assert!(
            resolve_endpoint(&po).is_err(),
            "Qwen 决策 provider 缺 base_url 必须显式失败，不得兜底到 TypeSafe 默认端点"
        );
        // 空白串与 None 同义（既有 filter 口径），也必须是 Err
        po.base_url = Some("   ".to_string());
        assert!(resolve_endpoint(&po).is_err());
    }

    /// 百炼 workspace 专用端点：base_url 承载 workspace id + 地域，client 只追加 `/systemone`
    ///
    /// 官方原文：`https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/compatible-mode/v1/systemone`
    /// （新加坡把 `cn-beijing` 换成 `ap-southeast-1`）。本用例钉两件事：
    /// ① workspace 段原样保留在 host 里（不被任何归一化吃掉）；② 版本前缀不重复叠加。
    #[test]
    fn qwen_decision_provider_uses_bailian_workspace_endpoint() {
        let mut po = provider("{}");
        po.provider_type = ProviderType::Qwen;
        po.base_url =
            Some("https://ws-abc123.cn-beijing.maas.aliyuncs.com/compatible-mode/v1".to_string());
        assert_eq!(
            resolve_endpoint(&po).unwrap(),
            "https://ws-abc123.cn-beijing.maas.aliyuncs.com/compatible-mode/v1/systemone"
        );
        // 尾斜杠容错（既有 trim_end_matches 口径不回归）
        po.base_url = Some(
            "https://ws-abc123.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1/".to_string(),
        );
        assert_eq!(
            resolve_endpoint(&po).unwrap(),
            "https://ws-abc123.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1/systemone"
        );
    }

    /// **两类型并存、各走各的**（任务书第 ④ 条）——正反成对（R54）：
    /// ① Jev 缺 base_url 仍走 TypeSafe 默认（旧形态未被新分派破坏）；
    /// ② Qwen 缺 base_url 是 Err（新形态按新规则生效）。
    /// 同一函数、同一配置输入、仅 provider_type 不同 —— 差异只能来自分派本身。
    #[test]
    fn jev_and_qwen_endpoints_do_not_cross_talk() {
        let jev = provider("{}"); // helper 内建 provider_type = Jev
        assert_eq!(jev.provider_type, ProviderType::Jev);
        assert_eq!(
            resolve_endpoint(&jev).unwrap(),
            "https://api.typesafe.ai/v1/systemone"
        );

        let mut qwen = provider("{}");
        qwen.provider_type = ProviderType::Qwen;
        assert!(resolve_endpoint(&qwen).is_err());

        // 反过来也成立：Qwen 配了 base_url 后正常，Jev 配同名 base_url 也正常（各自独立）
        qwen.base_url = Some("https://ws-x.cn-beijing.maas.aliyuncs.com/compatible-mode/v1".into());
        assert!(resolve_endpoint(&qwen).is_ok());
        let mut jev_custom = provider("{}");
        jev_custom.base_url = Some("https://relay.example.com/v1".into());
        assert_eq!(
            resolve_endpoint(&jev_custom).unwrap(),
            "https://relay.example.com/v1/systemone"
        );
    }

    /// 错误信息必须**指向真正要改的东西**（`base_url`）——否则运维只能看到
    /// 一个泛化的 401/DNS 错误（本单的核心动机之一）。
    #[test]
    fn missing_default_base_url_error_mentions_base_url() {
        let mut po = provider("{}");
        po.provider_type = ProviderType::Qwen;
        let e = resolve_endpoint(&po).unwrap_err();
        let text = format!("{e}");
        assert!(
            text.contains("base_url"),
            "错误信息须含 base_url 以便定位，实际: {text}"
        );
    }
}
