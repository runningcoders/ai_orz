//! 总结门（小脑决策点 P3-c / K6）
//!
//! 位置：`ConsolidateNode::run` 的「本轮结束后是否压缩存档」判定点。
//! 目的：**认出「机械落档就够」的轮次，省掉一次模型总结**，而不是改变记忆语义
//! （跳过后仍写短期记忆，只是质量降到原文截断，与直答路径同口径）。
//!
//! # 三态结论（跳过 = 降级到廉价路径，绝不产出业务结论）
//!
//! | 结论 | 触发 | 返回 |
//! |---|---|---|
//! | 不问门 | `would_compress == false`（本来就不压缩） | `None`，**零调用、零配额、零轨迹** |
//! | `skip` | 小脑判「机械落档就够」 | `Some(SkipSummary)` → 不压缩，改 `persist_turn_memory` |
//! | 照旧（`summarize`） | 判「值得模型总结」 | `None` → 调用方走原逻辑 |
//!
//! # 优先级（K6 定谳）
//!
//! `should_compress && !skip` —— 压缩判据在前、小脑只做**减法**：
//! 阈值策略说「这轮该压」才有机会被跳过；阈值策略说「不该压」时本门一次都不问
//! （否则每个直答轮都要多一次小脑调用，且 skip 率的分母会被「本来就不压」的轮次淹没）。
//! 本门**不能**把「不该压」变成「压」：省钱的门只许省钱。
//!
//! # 与 `persist_turn_memory` 的分工
//!
//! 跳过压缩 ≠ 不写记忆。跳过路径必须自己补上落档：`persist_turn_memory` 是那条
//! 路径上**唯一的持久化出口**（`insert_short_term` 的 `QUICK_TURN_TAG` 分支），
//! 因此「不压缩」可被计数为「TurnArchive 压缩 0 次」，而记忆条数不变。
//!
//! # 降级（业务方忽略、走原逻辑）
//!
//! 无小脑（未装配）/ 未装配 provider / 配额耗尽 / 调用失败 / 超时 / 低置信 /
//! 答案不可用 → 一律 `None`。小脑是可选增强，**失败只允许降级，不得阻断主流程**：
//! 本模块不出现 `unwrap` / `expect` / `?`。
//!
//! # 观测：轨迹是唯一载体
//!
//! 每次真正问门都写一条 `record_cerebellum{scene: "summary_gate", decision}`，
//! `decision` 三值 = `skip` / `summarize` / `fallback`。
//! **「不适用」不写轨迹**（未装配 provider / 本来不压缩）：它是「本组织没配小脑」
//! 或「这轮无需决策」，不是一次决策，混进分母会把跳过率算低。

use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::policy::{Metrics, PickOnePolicy, Policy};
use crate::pkg::think_trace::{ThinkTraceCerebellum, record_cerebellum};
use crate::service::dal::brain::BrainDal;
use crate::service::domain::runtime::cerebellum_context as context;
use std::sync::Arc;

/// 场景标识（日志 / 计量口径，也是 `DecisionRequest.scene`）
pub(crate) const SCENE: &str = "summary_gate";

/// 置信阈值：低于此值按「没结论」处理
///
/// 代价不对称：错判 `skip` 会把「含工具链中间过程」的轮次降级成原文截断
/// （细节丢失，下一轮追问时回忆不起路径/ID/报错），错判 `summarize` 只是
/// 多花一次模型总结 —— 故与入口门同口径 0.9。
pub(crate) const MIN_CONFIDENCE: f32 = 0.9;

/// 选项 id：跳过总结（机械落档）
pub(crate) const OPTION_SKIP: &str = "skip";
/// 选项 id：照旧用模型总结
pub(crate) const OPTION_SUMMARIZE: &str = "summarize";

/// 轨迹决策值：小脑判跳过（下游不发压缩，`applied=true`）
pub(crate) const DECISION_SKIP: &str = "skip";
/// 轨迹决策值：判照旧总结（下游与无本门时一致，`applied=false`）
pub(crate) const DECISION_SUMMARIZE: &str = "summarize";
/// 轨迹决策值：降级（`degraded` 给出原因）
pub(crate) const DECISION_FALLBACK: &str = "fallback";

/// 降级原因：本 run 小脑调用配额耗尽
const DEGRADED_BUDGET: &str = "budget_exhausted";
/// 降级原因：判了但没有可用结论（低置信 / 答案缺失 / 幻觉选项）
const DEGRADED_NO_ANSWER: &str = "no_answer";

/// 判据（prompt 存调用方，服务端不持有）
///
/// 只写黑名单侧：默认「照旧总结」，只有确定「机械落档就够」才 skip。
/// 三类**必须排除**在跳过面之外（宁可多压一次）：
/// ① 含工具链 / 查库 / 读文件 / 跑命令的轮次 —— 中间过程（路径、ID、报错）
///    原文截断会整段丢；
/// ② 多步任务的中间轮（「继续」「那就这么改」）—— 它短，但丢了上文就接不上；
/// ③ 拿不准 —— 误判 skip 的代价是「聊过就忘」，用户下次追问时记忆里没有细节。
const INSTRUCTIONS: &str = "请判断这一轮对话结束后，是否需要「用模型总结」来沉淀记忆：\
不需要总结，指这轮问的是一句话就能答完的事（寒暄、致谢、单句确认、简单事实问答），\
把原文截断写进记忆就够用。\n\
只有确定是这类才选 skip。其余一律选 summarize。特别注意：\n\
① 只要这轮涉及查数据、读文件、跑命令、多步推理，就选 summarize —— \
那些路径 / ID / 参数 / 报错一旦被原文截断就找不回来；\n\
② 多步任务里的中间一句（「继续」「那就这么改」）虽然短，但丢了上文就接不上，选 summarize；\n\
③ 拿不准就选 summarize —— 误判 skip 会让下一轮追问时「聊过就忘」，代价远大于多压一次。";

/// 两个选项（id → 判据描述）
const OPTIONS: [(&str, &str); 2] = [
    (
        OPTION_SUMMARIZE,
        "值得用模型总结沉淀：含工具链 / 多步推理 / 长文，或拿不准",
    ),
    (
        OPTION_SKIP,
        "机械落档就够：寒暄、致谢、单句确认、简单事实问答，原文截断即可",
    ),
];

/// 透传给模型的上下文事实：统一由 [`context::ContextBundle`] 渲染成单键分块正文
///
/// 键名收敛到 `context::STATE_KEY` 一处（单出口）：本文件不再出现
/// `agent_id` / `last_user_message` 这类裸键拼装。
const STATE_KEYS: &[&str] = &[context::STATE_KEY];

/// 跳过判决（零尺寸证据类型）
///
/// 刻意不用 `bool`：`Option<SkipSummary>` 让「没问出结论」「判照旧」「判跳过」
/// 三种情形在类型上就分得开，调用点不可能把 `None` 误当 `Some(false)` 处理。
pub(crate) struct SkipSummary;

/// 本场景的策略形态（无状态，可反复构造）
fn policy() -> PickOnePolicy {
    PickOnePolicy::new(
        SCENE,
        "本轮结束后是否跳过模型总结",
        "这轮对话结束后，机械落档（原文截断）是否够用，还是需要模型总结",
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

/// **唯一翻译点**：判决 → 「本轮是否走 TurnArchive 压缩」
///
/// 调用点只消费返回值，**不得自行翻译**（`nodes.rs` 里不出现
/// `Some(SkipSummary)` 的分支判断：一处翻译，判决语义变了只改这里）。
///
/// `None` 分支原样透传 `would_compress`（逐字节等价：没有小脑的组织行为不变），
/// `Some(SkipSummary)` 一律翻成 `false`（不压缩）—— 只能做减法，不能把
/// 「本来不压」翻成「压」。
pub(crate) fn apply(gate: Option<SkipSummary>, would_compress: bool) -> bool {
    match gate {
        Some(SkipSummary) => false,
        None => would_compress,
    }
}

/// 生产入口：判断本轮是否该跳过模型总结（`None` = 走原逻辑）
///
/// `would_compress` 是**阈值策略**的结论（`!hits.is_empty()`），必须先于本门：
/// 为 `false` 时一次都不问（零调用 / 零配额 / 零轨迹），这是 K6 的「优先级」。
/// `brain` 刻意收 `Option<&Brain>`：调用方直接传 `agent.brain.as_ref()`，缺失即降级。
pub(crate) async fn maybe_skip(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: Option<&Brain>,
    text: &str,
    trace_id: &str,
    would_compress: bool,
) -> Option<SkipSummary> {
    // ⓪ 本地可判的先判：阈值策略已经判「不该压」⇒ 无可跳过，直接不问。
    //    零调用、零配额、零轨迹 —— 「没问」与「问了没结论」必须在观测上分得开。
    if !would_compress {
        return None;
    }

    // ① 未装配小脑 = 降级，不是错误（本门绝不阻断主流程，也不 panic）
    //
    // 刻意写成 `let ... else` 而不是 `brain?`：`?` 在 `Option` 上下文里只是提前
    // `return None`，与 let-else 逐字节等价，不构成「把 None 升级成 Err / 中断
    // 主流程」；此处保留 let-else 只为让每一个降级点都在调用点显式可见。
    #[allow(clippy::question_mark)]
    let Some(brain) = brain else {
        return None;
    };

    // ② 未装配小脑 provider = **不适用（not applicable）**，不是降级：
    //    零请求、零配额、**零轨迹**，且不得落任何 `DECISION_*` 值 ——
    //    否则跳过率的分母会被「这个组织根本没配小脑」淹没。
    //
    //    与 `think_loop` 里 `brain.cerebellum.is_some()` 的判据同源；同时是集成
    //    测试的护栏（`Brain::new_local` 的 `cerebellum` 恒为 `None` ⇒ 不打网络）。
    // 同 ①：写成 if 而不是 `?`，是要让降级点在调用处显式可见。
    #[allow(clippy::question_mark)]
    if brain.cerebellum.is_none() {
        return None;
    }

    // ③ 本 run 的小脑调用配额（上限 = resolved 大脑轮次）：超限静默走原逻辑。
    //    **必须早于本门之后的任何记账**：放在配额之前会消耗 `used`，
    //    「calls == 0 && used 不变」那条断言会变成假绿。
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
    // 单出口（笔④-2 笔 B）：装配与容量自截都在组织器，本处只调一次
    let metrics = Metrics::new().with(context::STATE_KEY, context::render_for(brain, SCENE, text));

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
            "summary_gate",
            decision = DECISION_FALLBACK,
            degraded_reason = reason.as_str(),
            scene = SCENE,
            latency_ms = latency_ms,
            "summary gate degraded to original logic"
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
            "summary_gate",
            decision = DECISION_FALLBACK,
            degraded_reason = DEGRADED_NO_ANSWER,
            scene = SCENE,
            latency_ms = latency_ms,
            "summary gate produced no usable answer"
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
            "summary_gate",
            decision = DECISION_SKIP,
            confidence = pick.confidence,
            scene = SCENE,
            latency_ms = latency_ms,
            "turn summary skipped by cerebellum, falling back to quick store"
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
        return Some(SkipSummary);
    }

    // 照旧总结：下游行为与没有本门时一致，故 `applied=false`；留痕只为算
    // 跳过率（分母 = 本 scene 的轨迹条数，分子 = decision=skip）。
    log_info!(
        ctx,
        "summary_gate",
        decision = DECISION_SUMMARIZE,
        confidence = pick.confidence,
        scene = SCENE,
        latency_ms = latency_ms,
        "summary gate lets original compaction run"
    );
    record_cerebellum(
        trace_id,
        trace(
            DECISION_SUMMARIZE,
            Some(pick.confidence),
            None,
            false,
            traced_latency_ms,
        ),
    );
    None
}

#[cfg(test)]
#[path = "summary_gate_tests.rs"]
mod tests;
