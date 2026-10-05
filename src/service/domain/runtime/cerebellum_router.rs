//! 小脑路由器（B3 运行时快判断路由）
//!
//! 在 `run_think_loop` 首轮前做一次亚秒级小脑（System One decision 模型）快判断：
//! 让"琐碎请求"要么得到模板直回（默认关），要么带着路由结论进入 cortex 主循环
//! （System 增强提示，默认开）。三层降级全部收敛为「静默跳过 = 现状」：
//!
//! 1. 无启用小脑（`brain.cerebellum = None` / 总开关关）→ 全域静默跳过，
//!    同时即全局回滚开关（管理面停用默认小脑 = 一键回滚，零代码零重启）；
//! 2. 调用失败 / 超时（800ms）→ 静默跳过；
//! 3. 低置信 → 不注入、不直回。
//!
//! 三铁律口径：state 只携带轻量上下文（消息文本 + Agent 标识），不取全量记忆；
//! 结果只注入 cortex 增强，不替代主循环；单次调用受亚秒预算约束。
//!
//! 阈值与降级常量集中在此，便于 Spike 后集中调参（只调常量不动结构）。

use crate::models::cerebellum_types::{
    CerebellumAnswer, CerebellumQuestion, QuestionCriteria, QuestionType, ThinkFastResult,
};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::service::dal::brain::BrainDal;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

// ==================== 常量集中（Spike 后只调这里） ====================

/// 路由快判断超时（毫秒）：亚秒预算；dao 内部另有 per-request timeout 兜底
pub const CEREBELLUM_ROUTE_TIMEOUT_MS: u64 = 800;
/// 置信度阈值：低于该值不注入、不直回（保守口径，Spike 后可调）
pub const CEREBELLUM_ROUTE_CONFIDENCE_THRESHOLD: f32 = 0.85;

/// 路由问题 id（answers 键）与两个选项
pub const ROUTE_QUESTION_ID: &str = "route";
pub const CHOICE_TRIVIAL_DIRECT: &str = "trivial_direct";
pub const CHOICE_CORTEX_NEEDED: &str = "cortex_needed";

/// 模板直回文案（TRIVIAL 直回默认关；放开需 Spike 实测 + 灰度后另行拍板）
pub const TRIVIAL_DIRECT_TEMPLATE: &str = "收到。这个问题比较简单直接，我直接答复：目前没有需要深度处理的任务。如果你有具体需求（查数据、改配置、跑任务），直接说需求即可。";

/// 注入 cortex 首轮的增强 System 提示（默认开；纯增强，失败即现状）
pub const CORTEX_BOOST_TEMPLATE: &str =
    "[小脑路由提示] 快判断认为本请求需要完整思考能力，请按你的正常流程高质量完成本轮任务。";

// ==================== 决策类型 ====================

/// 降级原因（B5 观测口径；Skip 决策必带）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DegradedReason {
    /// 无启用小脑 / 总开关关闭（全局回滚开关形态）
    NoCerebellum,
    /// 无用户消息（无法构造轻量上下文）
    NoUserMessage,
    /// 调用失败（网络 / 协议 / 服务端）
    CallError,
    /// 超时（> CEREBELLUM_ROUTE_TIMEOUT_MS）
    Timeout,
    /// 低置信（< CEREBELLUM_ROUTE_CONFIDENCE_THRESHOLD）
    LowConfidence,
    /// 答案形态不符合预期（非 choice / 未知选项）
    UnsupportedAnswer,
}

impl DegradedReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            DegradedReason::NoCerebellum => "no_cerebellum",
            DegradedReason::NoUserMessage => "no_user_message",
            DegradedReason::CallError => "call_error",
            DegradedReason::Timeout => "timeout",
            DegradedReason::LowConfidence => "low_confidence",
            DegradedReason::UnsupportedAnswer => "unsupported_answer",
        }
    }
}

/// 路由决策三分类
#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// 琐碎请求：直回候选（是否真直回由调用方按 cerebellum_trivial_direct 开关裁决）
    Direct { content: String, confidence: f32 },
    /// 需要完整思考：注入增强 System 提示后进主循环
    Inject {
        system_prompt: String,
        confidence: f32,
    },
    /// 静默跳过 = 现状（降级原因用于 B5 观测）
    Skip { reason: DegradedReason },
}

// ==================== 入口 ====================

/// B3 运行时快判断路由
///
/// - `dal` 参数化 trait 对象：生产 = `dal::brain::dal()`（BrainDal::think_fast），测试注入 mock；
/// - `total` 开关关 / cerebellum None → Skip{NoCerebellum}（零调用，全局回滚开关）；
/// - state 只携带轻量上下文（last_user_message + agent 标识），不取全量记忆；
/// - 单次调用受 800ms 超时约束，任何失败/低置信 → Skip（= 现状）。
pub async fn route(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    cerebellum: &ModelProviderPo,
    enabled: bool,
    agent_id: &str,
    agent_name: &str,
    last_user_message: Option<&str>,
) -> RouteDecision {
    if !enabled {
        return RouteDecision::Skip {
            reason: DegradedReason::NoCerebellum,
        };
    }
    let Some(user_text) = last_user_message else {
        return RouteDecision::Skip {
            reason: DegradedReason::NoUserMessage,
        };
    };

    // 轻量上下文（三铁律：不取全量记忆、先于小脑的仅有消息文本）
    let state = json!({
        "agent_id": agent_id,
        "agent_name": agent_name,
        "last_user_message": user_text,
    });

    // 单 choice 路由问题
    let mut criteria = BTreeMap::new();
    criteria.insert(
        CHOICE_TRIVIAL_DIRECT.to_string(),
        "琐碎请求：寒暄/客套/简单确认类，无需工具与深度思考，可直接模板答复".to_string(),
    );
    criteria.insert(
        CHOICE_CORTEX_NEEDED.to_string(),
        "需要完整思考：具体任务/查数据/改配置/多轮指代等，必须进入主循环".to_string(),
    );
    let mut questions = BTreeMap::new();
    questions.insert(
        ROUTE_QUESTION_ID.to_string(),
        CerebellumQuestion {
            question_type: QuestionType::Choice,
            instructions: "根据用户消息判断应走模板直回还是完整思考主循环".to_string(),
            criteria: Some(QuestionCriteria::Choices(criteria)),
        },
    );
    if let Err(e) = questions
        .get(ROUTE_QUESTION_ID)
        .expect("route question just inserted")
        .validate()
    {
        log_warn!(ctx, "cerebellum_route", error = ?e, "route question validate failed, skipping");
        return RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer,
        };
    }

    // 亚秒预算：tokio 超时包裹；dao 内部另有 per-request timeout 兜底
    let started = Instant::now();
    let call = tokio::time::timeout(
        Duration::from_millis(CEREBELLUM_ROUTE_TIMEOUT_MS),
        dal.think_fast(ctx.clone(), cerebellum, state, questions),
    )
    .await;

    let (result, degraded) = match call {
        Err(_) => (None, Some(DegradedReason::Timeout)),
        Ok(Err(e)) => {
            log_warn!(ctx, "cerebellum_route", error = ?e, "think_fast call failed, skipping");
            (None, Some(DegradedReason::CallError))
        }
        Ok(Ok(r)) => (Some(r), None),
    };
    let latency_ms = started.elapsed().as_millis() as u64;

    let Some(result) = result else {
        let reason = degraded.unwrap_or(DegradedReason::CallError);
        log_info!(
            ctx,
            "cerebellum_route",
            decision = "skip",
            degraded_reason = reason.as_str(),
            latency_ms = latency_ms,
            "cerebellum route degraded"
        );
        return RouteDecision::Skip { reason };
    };

    let decision = translate(&result, latency_ms);
    match &decision {
        RouteDecision::Direct {
            content,
            confidence,
        } => {
            log_info!(
                ctx,
                "cerebellum_route",
                decision = "direct",
                confidence = confidence,
                latency_ms = latency_ms,
                input_tokens = result.usage.input_tokens,
                output_tokens = result.usage.output_tokens,
                content_chars = content.len(),
                "cerebellum route decision"
            );
        }
        RouteDecision::Inject { confidence, .. } => {
            log_info!(
                ctx,
                "cerebellum_route",
                decision = "inject",
                confidence = confidence,
                latency_ms = latency_ms,
                input_tokens = result.usage.input_tokens,
                output_tokens = result.usage.output_tokens,
                "cerebellum route decision"
            );
        }
        RouteDecision::Skip { reason } => {
            log_info!(
                ctx,
                "cerebellum_route",
                decision = "skip",
                degraded_reason = reason.as_str(),
                latency_ms = latency_ms,
                input_tokens = result.usage.input_tokens,
                output_tokens = result.usage.output_tokens,
                "cerebellum route decision"
            );
        }
    }
    decision
}

/// 答案 → 决策翻译（纯函数，单测覆盖三分类）
///
/// 规则：cortex_needed 且置信达标 → Inject；trivial_direct 且置信达标 → Direct；
/// 其余（非 choice / 未知选项 / 低置信）→ Skip。
fn translate(result: &ThinkFastResult, _latency_ms: u64) -> RouteDecision {
    let Some(answer) = result.answers.get(ROUTE_QUESTION_ID) else {
        return RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer,
        };
    };
    let CerebellumAnswer::Choice {
        choice, confidence, ..
    } = answer
    else {
        return RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer,
        };
    };
    if *confidence < CEREBELLUM_ROUTE_CONFIDENCE_THRESHOLD {
        return RouteDecision::Skip {
            reason: DegradedReason::LowConfidence,
        };
    }
    match choice.as_str() {
        CHOICE_CORTEX_NEEDED => RouteDecision::Inject {
            system_prompt: CORTEX_BOOST_TEMPLATE.to_string(),
            confidence: *confidence,
        },
        CHOICE_TRIVIAL_DIRECT => RouteDecision::Direct {
            content: TRIVIAL_DIRECT_TEMPLATE.to_string(),
            confidence: *confidence,
        },
        _ => RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer,
        },
    }
}

/// 便捷封装：单例 Brain DAL（BrainDal::think_fast）的生产入口
pub async fn route_with_default_dao(
    ctx: &RequestContext,
    cerebellum: &ModelProviderPo,
    enabled: bool,
    agent_id: &str,
    agent_name: &str,
    last_user_message: Option<&str>,
) -> RouteDecision {
    route(
        ctx,
        crate::service::dal::brain::dal(),
        cerebellum,
        enabled,
        agent_id,
        agent_name,
        last_user_message,
    )
    .await
}
#[cfg(test)]
#[path = "cerebellum_router_tests.rs"]
mod tests;
