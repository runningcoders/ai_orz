//! 意图分析前的成本门（小脑决策点 P3-a · 定谳 K4-b）
//!
//! 位置：`analyze_input_intent_inner` 入口，Phase 1（意图分析）之前。
//! 目的：**省掉明显不值得跑的意图分析**，而不是改变意图分析的行为。
//!
//! # 三态结论（跳过 = 降级到现状，绝不产出业务结论）
//!
//! | 结论 | 触发 | 返回 |
//! |---|---|---|
//! | `empty_input` | 输入 `trim()` 后为空（零信息量） | `Some(空 IntentAnalysis)`，**零调用、零配额** |
//! | `skip` | 小脑判「不值得做意图分析」 | `Some(IntentAnalysis { key_terms = [用户原文] })` |
//! | 放行（`analyze`） | 判「值得做」 | `None` → 调用方走原逻辑 |
//!
//! 跳过路径只写 `key_terms`（整句用户原文，不拆词），由 `retrieve_node` 的既有
//! 机制登记进 `queried_terms`；不填 `direct_answer`、不填 `need_clarification`，
//! 因此既有的两条短路（直答 / 澄清）都不会被触发。
//!
//! # 降级（业务方忽略、走原逻辑）
//!
//! 无小脑（未装配）/ 配额耗尽 / 调用失败 / 超时 / 低置信 / 答案不可用 → 一律
//! `None`。小脑是可选增强，**失败只允许降级，不得阻断主流程**：本模块不出现
//! `unwrap` / `expect` / `?`，`brain` 缺失也按降级处理，不是错误。
//!
//! # 观测：轨迹是唯一载体
//!
//! 每次进入本门都写一条 `record_cerebellum{scene: "intent_gate", decision}`，
//! `decision` 四值 = `empty_input` / `skip` / `analyze` / `fallback`。
//! 统计口径就在这一处区分（本地判空与模型判低价值不得混算），AOP 的
//! `started/finished` status 一律不动 —— skip 是正常决策不是异常，往 status
//! 里塞 `skipped` 会让成功率/失败率统计失真，且要新增枚举值波及前端与统计侧。

use super::types::IntentAnalysis;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::policy::{Metrics, PickOnePolicy, Policy};
use crate::pkg::think_trace::{ThinkTraceCerebellum, record_cerebellum};
use crate::service::dal::brain::{BrainDal, dal};
use std::sync::Arc;

/// 场景标识（日志 / 计量口径，也是 `DecisionRequest.scene`）
pub(crate) const SCENE: &str = "intent_gate";

/// 置信阈值：低于此值按「没结论」处理
///
/// 与入口门同口径 0.9：代价不对称 —— 错判 `skip` 会让后续检索丢掉关键词
/// （整轮质量下降），错判 `analyze` 只是多花一次意图分析。
pub(crate) const MIN_CONFIDENCE: f32 = 0.9;

/// 选项 id：放行（做意图分析）
pub(crate) const OPTION_ANALYZE: &str = "analyze";
/// 选项 id：跳过（不做意图分析）
pub(crate) const OPTION_SKIP: &str = "skip";

/// 轨迹决策值：本地判「空输入」（零调用，区别于模型判低价值）
pub(crate) const DECISION_EMPTY_INPUT: &str = "empty_input";
/// 轨迹决策值：模型判跳过
pub(crate) const DECISION_SKIP: &str = "skip";
/// 轨迹决策值：放行（下游行为与无本门时一致，`applied=false`）
pub(crate) const DECISION_ANALYZE: &str = "analyze";
/// 轨迹决策值：降级（`degraded` 给出原因）
pub(crate) const DECISION_FALLBACK: &str = "fallback";

/// 降级原因：本 run 小脑调用配额耗尽
const DEGRADED_BUDGET: &str = "budget_exhausted";
/// 降级原因：判了但没有可用结论（低置信 / 答案缺失 / 幻觉选项）
const DEGRADED_NO_ANSWER: &str = "no_answer";

/// 判据（prompt 存调用方，服务端不持有）
///
/// 只写黑名单侧：默认「值得做」，只有零信息量才跳。
/// 两类**必须排除**在跳过面之外（宁可多做一次分析）：
/// ① 寒暄 / 致谢 / 感慨 —— 它们跑意图分析常能命中直答短路，跳过反而更贵；
/// ② 纯标点（「？」「。。。」）—— 形态判不出语义，常是真提问的前导。
const INSTRUCTIONS: &str = "请判断这条用户消息是否值得做意图分析（抽取意图类型 / 关键词 / \
指代消歧 / 是否需要澄清）。
只有「零信息量」才选 skip：纯标点或空白、纯表情，或「好」「嗯」「收到」「OK」这类单字超短确认。
其余一律选 analyze。特别注意两点：
① 寒暄、致谢、感慨不要选 skip —— 这类消息跑意图分析常能直接命中直答，跳过反而更贵；
② 拿不准就选 analyze —— 漏做意图分析会让后续检索拿不到关键词，代价远大于多做一次分析。";

/// 两个选项（id → 判据描述）
const OPTIONS: [(&str, &str); 2] = [
    (
        OPTION_ANALYZE,
        "需要做意图分析：消息里有实际诉求（提问 / 任务 / 澄清 / 承接上文的指代）",
    ),
    (
        OPTION_SKIP,
        "不必做意图分析：零信息量输入（纯标点或空白、纯表情），或「好」「嗯」\
「收到」「OK」这类单字超短确认",
    ),
];

/// 透传给模型的上下文事实（与路由同口径：只带轻量上下文，不取全量记忆）
const STATE_KEYS: &[&str] = &["agent_id", "agent_name", "last_user_message"];

/// 本场景的策略形态（无状态，可反复构造）
fn policy() -> PickOnePolicy {
    PickOnePolicy::new(
        SCENE,
        "意图分析价值门",
        "这条用户消息值不值得做意图分析（意图类型 / 关键词 / 指代消歧 / 澄清）",
        INSTRUCTIONS,
        OPTIONS,
        MIN_CONFIDENCE,
    )
    .with_state_keys(STATE_KEYS)
}

/// 一条小脑留痕（三态 + 降级共用一个构造器，避免字段漏填）
fn trace(
    decision: &str,
    confidence: Option<f32>,
    degraded: Option<&str>,
    applied: bool,
    latency_ms: Option<u64>,
) -> ThinkTraceCerebellum {
    ThinkTraceCerebellum {
        scene: SCENE.to_string(),
        decision: decision.to_string(),
        confidence,
        degraded: degraded.map(|s| s.to_string()),
        applied,
        latency_ms,
    }
}

/// 跳过路径产出的意图结论（**不是`默认值`，是「跳过后的等价现状」**）
///
/// K4-b 定谳：跳过时只有用户原文可用，整句当检索词（不拆词）。整句的语义召回
/// 低于拆词是已知代价 —— 但在这里拆词就等于在门里偷偷做一遍意图分析，本末倒置。
fn skipped_analysis(text: &str) -> IntentAnalysis {
    IntentAnalysis {
        key_terms: if text.trim().is_empty() {
            Vec::new()
        } else {
            vec![text.to_string()]
        },
        ..Default::default()
    }
}

/// 生产入口：判断本次是否该跳过意图分析（`None` = 走原逻辑）
///
/// `brain` 刻意收 `Option<&Brain>`：调用方直接传 `agent.brain()`，缺失即降级。
pub(crate) async fn skip_if_pointless(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: Option<&Brain>,
    text: &str,
    trace_id: &str,
) -> Option<IntentAnalysis> {
    // ① 零信息量：本地判 —— 不占配额、不发请求。
    //    判定面严格限 `trim()` 后为空：纯标点仍送小脑判，别让本地黑名单和
    //    小脑黑名单变成两套真相（本地只兜「形态上零信息量」那一种）。
    if text.trim().is_empty() {
        record_cerebellum(
            trace_id,
            trace(DECISION_EMPTY_INPUT, None, None, true, None),
        );
        log_info!(
            ctx,
            "intent_gate",
            decision = DECISION_EMPTY_INPUT,
            "intent analysis skipped: empty input (zero call, zero quota)"
        );
        return Some(skipped_analysis(text));
    }

    // ② 未装配小脑 = 降级，不是错误（本门绝不阻断主流程，也不 panic）
    //
    // 刻意写成 `let ... else` 而不是 `brain?`：`?` 在 `Option` 上下文里只是提前
    // `return None`，与 let-else 逐字节等价，不构成「把 None 升级成 Err / 中断主流程」；
    // 此处保留 let-else 只为让每一个降级点都在调用点显式可见，不被 `?` 藏进表达式。
    // 若本函数返回类型改为 `Result`，`?` 才真变成错误升级，此 allow 必须重审。
    #[allow(clippy::question_mark)]
    let Some(brain) = brain else {
        return None;
    };

    // ③ 本 run 的小脑调用配额（上限 = resolved 大脑轮次）：超限静默走原逻辑
    if !brain.cerebellum_quota.try_acquire() {
        log_info!(
            ctx,
            "cerebellum_quota",
            scene = SCENE,
            limit = brain.cerebellum_quota.limit(),
            used = brain.cerebellum_quota.used(),
            "cerebellum call budget exhausted, fall back to original logic"
        );
        record_cerebellum(
            trace_id,
            trace(DECISION_FALLBACK, None, Some(DEGRADED_BUDGET), false, None),
        );
        return None;
    }

    let policy = policy();
    let metrics = Metrics::new()
        .with("agent_id", &brain.agent_id)
        .with("agent_name", &brain.agent_name)
        .with("last_user_message", text);

    // 阶段一：策略声明要问什么（纯函数；未命中就一次调用都不发）
    let requests = policy.prepare(&metrics);
    if requests.is_empty() {
        return None;
    }

    // 阶段二：唯一 async 点（门禁 / 超时 / 协议转换 / 降级全在 DAL）
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;
    // 轨迹口径：`None` = 本次没真正发出请求（空请求门 / 三个零调用门）
    let traced_latency_ms = outcome.traced_latency_ms();

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "intent_gate",
            decision = DECISION_FALLBACK,
            degraded_reason = reason.as_str(),
            scene = SCENE,
            latency_ms = latency_ms,
            "intent gate degraded to original logic"
        );
        record_cerebellum(
            trace_id,
            trace(
                DECISION_FALLBACK,
                None,
                Some(reason.as_str()),
                false,
                traced_latency_ms,
            ),
        );
        return None;
    }

    // 阶段三：答案注入事实桶，形态负责「选一个 + 置信达标 + 不采信幻觉选项」
    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let Some(pick) = policy.chosen(&metrics) else {
        log_info!(
            ctx,
            "intent_gate",
            decision = DECISION_FALLBACK,
            degraded_reason = DEGRADED_NO_ANSWER,
            scene = SCENE,
            latency_ms = latency_ms,
            "intent gate produced no usable answer"
        );
        record_cerebellum(
            trace_id,
            trace(
                DECISION_FALLBACK,
                None,
                Some(DEGRADED_NO_ANSWER),
                false,
                traced_latency_ms,
            ),
        );
        return None;
    };

    if pick.option == OPTION_SKIP {
        log_info!(
            ctx,
            "intent_gate",
            decision = DECISION_SKIP,
            confidence = pick.confidence,
            scene = SCENE,
            latency_ms = latency_ms,
            input_tokens = outcome.usage.input_tokens,
            output_tokens = outcome.usage.output_tokens,
            "intent analysis skipped by cerebellum"
        );
        record_cerebellum(
            trace_id,
            trace(
                DECISION_SKIP,
                Some(pick.confidence),
                None,
                true,
                traced_latency_ms,
            ),
        );
        return Some(skipped_analysis(text));
    }

    // 放行：下游行为与没有本门时一致，故 `applied=false`；留痕只为算跳过率
    // （分母 = 本 scene 的轨迹条数，分子 = decision=skip + empty_input）。
    log_info!(
        ctx,
        "intent_gate",
        decision = DECISION_ANALYZE,
        confidence = pick.confidence,
        scene = SCENE,
        latency_ms = latency_ms,
        "intent gate lets original path run"
    );
    record_cerebellum(
        trace_id,
        trace(
            DECISION_ANALYZE,
            Some(pick.confidence),
            None,
            false,
            traced_latency_ms,
        ),
    );
    None
}

/// 单例 DAL 的生产封装
pub(crate) async fn skip_if_pointless_with_default_dao(
    ctx: &RequestContext,
    brain: Option<&Brain>,
    text: &str,
    trace_id: &str,
) -> Option<IntentAnalysis> {
    skip_if_pointless(ctx, dal(), brain, text, trace_id).await
}

#[cfg(test)]
#[path = "intent_gate_tests.rs"]
mod tests;
