//! 小脑路由器（B3 运行时快判断路由）
//!
//! 在 `run_think_loop` 首轮前做一次亚秒级小脑（System One decision 模型）快判断：
//! 让"琐碎请求"要么得到模板直回（默认关），要么带着路由结论进入 cortex 主循环
//! （System 增强提示，默认开）。三层降级全部收敛为「静默跳过 = 现状」：
//!
//! 1. 无启用小脑（`brain.cerebellum = None` / 总开关关）→ 全域静默跳过，
//!    同时即全局回滚开关（管理面停用默认小脑 = 一键回滚，零代码零重启）；
//! 2. 调用失败 / 超时（`provider.config.timeout_ms`，缺省 800ms）→ 静默跳过；
//! 3. 低置信 → 不注入、不直回。
//!
//! 三铁律口径：state 只携带轻量上下文（消息文本 + Agent 标识），不取全量记忆；
//! 结果只注入 cortex 增强，不替代主循环；单次调用受亚秒预算约束。
//!
//! 「问什么」不在这里写死：[`route_policy`] 用 [`PickOnePolicy`] 形态声明一次单选，
//! 本模块只负责「闸门 → 交给 DAL → 把结论翻成路由三分类」。
//!
//! 阈值与降级常量集中在此，便于 Spike 后集中调参（只调常量不动结构）。

use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::policy::{DecisionPolicy, DecisionValue, Metrics, PickOne, PickOnePolicy, Policy};
use crate::service::dal::brain::BrainDal;
use common::api::cerebellum::DegradedReason;
use std::sync::Arc;

// ==================== 常量集中（Spike 后只调这里） ====================

/// 置信度阈值：低于该值不注入、不直回（保守口径，Spike 后可调）
pub const CEREBELLUM_ROUTE_CONFIDENCE_THRESHOLD: f32 = 0.85;

/// 场景名（日志/计量口径，也是 qid 命名空间前缀）
pub const ROUTE_SCENE: &str = "route";

/// 两个选项
pub const CHOICE_TRIVIAL_DIRECT: &str = "trivial_direct";
pub const CHOICE_CORTEX_NEEDED: &str = "cortex_needed";

/// 判据（prompt 存调用方，服务端不持有）
const ROUTE_INSTRUCTIONS: &str = "根据用户消息判断应走模板直回还是完整思考主循环";

/// 透传给模型的上下文事实（三铁律：只带轻量上下文，不取全量记忆）
const ROUTE_STATE_KEYS: &[&str] = &["agent_id", "agent_name", "last_user_message"];

/// 模板直回文案（TRIVIAL 直回默认关；放开需 Spike 实测 + 灰度后另行拍板）
pub const TRIVIAL_DIRECT_TEMPLATE: &str = "收到。这个问题比较简单直接，我直接答复：目前没有需要深度处理的任务。如果你有具体需求（查数据、改配置、跑任务），直接说需求即可。";

/// 注入 cortex 首轮的增强 System 提示（默认开；纯增强，失败即现状）
pub const CORTEX_BOOST_TEMPLATE: &str =
    "[小脑路由提示] 快判断认为本请求需要完整思考能力，请按你的正常流程高质量完成本轮任务。";

// ==================== 决策类型 ====================

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

/// 路由形态：一次单选，选项固定
///
/// 三铁律之一（只带轻量上下文）体现为 `with_state_keys`：`Metrics` 里可以有很多事实，
/// 但透传给小脑的只有声明过的那几项。
fn route_policy() -> PickOnePolicy {
    PickOnePolicy::new(
        ROUTE_SCENE,
        "小脑路由",
        "首轮快速判断是否需要完整思考",
        ROUTE_INSTRUCTIONS,
        [
            (
                CHOICE_TRIVIAL_DIRECT,
                "琐碎请求：寒暄/客套/简单确认类，无需工具与深度思考，可直接模板答复",
            ),
            (
                CHOICE_CORTEX_NEEDED,
                "需要完整思考：具体任务/查数据/改配置/多轮指代等，必须进入主循环",
            ),
        ],
        CEREBELLUM_ROUTE_CONFIDENCE_THRESHOLD,
    )
    .with_state_keys(ROUTE_STATE_KEYS)
}

/// B3 运行时快判断路由
///
/// - `dal` 参数化 trait 对象：生产 = `dal::brain::dal()`（BrainDal::decide），测试注入 mock；
/// - `enabled=false` → Skip{Disabled}（零调用，全局回滚开关）；
/// - provider 未装配 / 未配好 → 由 DAL 返回 `NoProvider`，本层不判空。
///   ⚠️ 生产路径 `think_loop` 外面套了 `brain.cerebellum.is_some()`，所以现场
///   只可能看到「已装配但 api_key 空」这一种 `NoProvider`；「未装配」根本进不来。
/// - state 只携带轻量上下文（last_user_message + agent 标识），不取全量记忆；
/// - 超时 / 协议转换 / 降级全在 DAL，任何失败 → Skip（= 现状）。
pub async fn route(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: &Brain,
    enabled: bool,
    last_user_message: Option<&str>,
) -> RouteDecision {
    if !enabled {
        return RouteDecision::Skip {
            reason: DegradedReason::Disabled,
        };
    }
    let Some(user_text) = last_user_message else {
        return RouteDecision::Skip {
            reason: DegradedReason::NoUserMessage,
        };
    };

    // 轻量上下文（三铁律：不取全量记忆、先于小脑的仅有消息文本）
    let policy = route_policy();
    let metrics = Metrics::new()
        .with("agent_id", &brain.agent_id)
        .with("agent_name", &brain.agent_name)
        .with("last_user_message", user_text);

    // 形态负责「问什么」：一次单选，选项与判据都在策略里；
    // 选项描述只写判据语义，不写实现细节——换模型 / 换实现时判据不用改。
    let requests = policy.prepare(&metrics);

    // 门禁、超时、协议转换、降级全部在 DAL（BrainDal::decide）内完成；
    // 这一层只负责「问什么」与「答案怎么用」。
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "cerebellum_route",
            decision = "skip",
            degraded_reason = reason.as_str(),
            latency_ms = latency_ms,
            "cerebellum route degraded"
        );
        return RouteDecision::Skip { reason };
    }

    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let decision = translate(&policy, &metrics);
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
                input_tokens = outcome.usage.input_tokens,
                output_tokens = outcome.usage.output_tokens,
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
                input_tokens = outcome.usage.input_tokens,
                output_tokens = outcome.usage.output_tokens,
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
                input_tokens = outcome.usage.input_tokens,
                output_tokens = outcome.usage.output_tokens,
                "cerebellum route decision"
            );
        }
    }
    decision
}

/// 决策结论 → 路由三分类（纯函数，单测覆盖三分类）
///
/// 规则：cortex_needed 且置信达标 → Inject；trivial_direct 且置信达标 → Direct；
/// 其余（无结论 / 非 Pick / 未知选项 / 低置信）→ Skip。
///
/// ⚠️ `None`（没问出来）必须走 Skip，绝不能落到 Direct/Inject ——
/// 「降级 = 未命中」，把「没判断出来」当成「判断为琐碎」会静默改变主流程。
fn translate(policy: &PickOnePolicy, metrics: &Metrics) -> RouteDecision {
    let Some(PickOne { option, confidence }) =
        policy.pick_one(metrics, CEREBELLUM_ROUTE_CONFIDENCE_THRESHOLD)
    else {
        // 区分「答了但不可用」的两种口径：Pick 但置信不足 → 低置信；
        // 其它（没答 / 题型不符 / 模型编造）→ 形态不符。
        let reason = match metrics.decision(&policy.primary_qid()) {
            Some(DecisionValue::Pick { .. }) => DegradedReason::LowConfidence,
            _ => DegradedReason::UnsupportedAnswer,
        };
        return RouteDecision::Skip { reason };
    };
    match option.as_str() {
        CHOICE_CORTEX_NEEDED => RouteDecision::Inject {
            system_prompt: CORTEX_BOOST_TEMPLATE.to_string(),
            confidence,
        },
        CHOICE_TRIVIAL_DIRECT => RouteDecision::Direct {
            content: TRIVIAL_DIRECT_TEMPLATE.to_string(),
            confidence,
        },
        _ => RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer,
        },
    }
}

/// 便捷封装：单例 Brain DAL（BrainDal::decide）的生产入口
///
/// 前置门（provider 未装配 / api_key 空 / DAO 单例未初始化）已全部下沉到
/// `BrainDal::decide`——那里是唯一执行点，重复判空只会让两处口径慢慢分叉。
/// 本函数只取单例 + 透传。
pub async fn route_with_default_dao(
    ctx: &RequestContext,
    brain: &Brain,
    enabled: bool,
    last_user_message: Option<&str>,
) -> RouteDecision {
    route(
        ctx,
        crate::service::dal::brain::dal(),
        brain,
        enabled,
        last_user_message,
    )
    .await
}
#[cfg(test)]
#[path = "cerebellum_router_tests.rs"]
mod tests;
