//! 唤醒后的成本门（小脑决策点 P3-b）
//!
//! 位置：`DeliberateNode::run` 入口，本轮第一次思考之前。
//! 目的：**认出「一眼能答」的消息，只给它一轮思考、强度真 0**，而不是改变
//! 思考本身的能力（工具、退出策略、收尾流程全部照旧）。
//!
//! # 三态结论（跳过 = 降级到现状，绝不产出业务结论）
//!
//! | 结论 | 触发 | 返回 |
//! |---|---|---|
//! | 不问门 | `total_rounds > 0`（本 run 已经跑过轮次） | `None`，**零调用、零配额、零轨迹** |
//! | `shallow` | 小脑判「一眼可答」 | `Some(Shallow)` → `apply` 后 1 轮 + 强度 0 |
//! | 放行（`deep`） | 判「需要完整思考」 | `None` → 调用方走原逻辑 |
//!
//! 浅答不是「少做点事」，而是「省掉不必要的事」：判浅答后仍走完整的
//! deliberate 循环（同一套工具、退出策略、收尾），只是 `max_rounds = 1`。
//!
//! # 为什么强度要独立通道
//!
//! 思考强度是 `base + 场景偏移 + 动态偏移`（**加减，不是赋值**）：`base = 70`
//! 时靠偏移减不到 0，用偏移表达「浅答强度 = 0」是假语义。所以
//! [`apply`] 返回 `force_intensity = Some(ThinkingIntensity::MIN)`，由
//! `resolve_call_options` 在 **Provider 能力开关的 map 闭包内**直接采用：
//! `None` 时整条 base/offset 链逐字节等价，`Some` 才是真 0，且 Provider 未配
//! `thinking_intensity` 时本通道也不得绕过能力开关。
//!
//! # 门闩取 run 级轮次，不取周期内起始轮次
//!
//! 判据是 `read_budget(bus).total_rounds`（跨压缩 / 跨周期累计）== 0，即
//! **整次唤醒的第一轮**。`poll_exit` 续跑复用同一 run ⇒ 续跑轮
//! `total_rounds > 0` ⇒ 天然不重走门、判决沿首轮结果（这是被接受的后果，
//! 不是缺陷：续跑追加的是同一任务的后续输入，不该每次被重新判一遍要不要浅答）。
//! 该取值**不来自** `start_round` —— 本模块签名里没有它，调用点实参必须是
//! `read_budget(bus).total_rounds`（review 级静态可读要求，见 `nodes.rs`）。
//!
//! # 降级（业务方忽略、走原逻辑）
//!
//! 无小脑（未装配）/ 配额耗尽 / 调用失败 / 超时 / 低置信 / 答案不可用 → 一律
//! `None`。小脑是可选增强，**失败只允许降级，不得阻断主流程**：本模块不出现
//! `unwrap` / `expect` / `?`，`brain` 缺失也按降级处理，不是错误。
//!
//! # 观测：轨迹是唯一载体
//!
//! 每次真正问门都写一条 `record_cerebellum{scene: "wake_gate", decision}`，
//! `decision` 四值 = `shallow` / `deep` / `fallback`（非首轮不问门 ⇒ 不写轨迹，
//! 否则浅答率的分母会被续跑轮重复计入）。

use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::policy::{Metrics, PickOnePolicy, Policy};
use crate::pkg::think_trace::{ThinkTraceCerebellum, record_cerebellum};
use crate::service::dal::brain::BrainDal;
use crate::service::domain::runtime::cerebellum_context as context;
use common::enums::ThinkingIntensity;
use std::sync::Arc;

/// 场景标识（日志 / 计量口径，也是 `DecisionRequest.scene`）
///
/// 与 `consumer::wake_gate_policy`（消费者层「任务依赖未满足不唤醒」）**同名
/// 不同物**：那个不调小脑、不看消息内容，是本模块之外的既有机制。
pub(crate) const SCENE: &str = "wake_gate";

/// 置信阈值：低于此值按「没结论」处理
///
/// 代价不对称：错判 `shallow` 会把用户的多步任务砍成一句一轮回复（用户直接
/// 收不到答案），错判 `deep` 只是多跑几轮完整思考 —— 故与入口门同口径 0.9。
pub(crate) const MIN_CONFIDENCE: f32 = 0.9;

/// 选项 id：浅答（1 轮 + 强度 0）
pub(crate) const OPTION_SHALLOW: &str = "shallow";
/// 选项 id：深答（完全走原逻辑）
pub(crate) const OPTION_DEEP: &str = "deep";

/// 轨迹决策值：小脑判浅答（下游行为被改变，`applied=true`）
pub(crate) const DECISION_SHALLOW: &str = "shallow";
/// 轨迹决策值：小脑判深答（下游行为与无本门时一致，`applied=false`）
pub(crate) const DECISION_DEEP: &str = "deep";
/// 轨迹决策值：降级（`degraded` 给出原因）
pub(crate) const DECISION_FALLBACK: &str = "fallback";

/// 降级原因：本 run 小脑调用配额耗尽
const DEGRADED_BUDGET: &str = "budget_exhausted";
/// 降级原因：判了但没有可用结论（低置信 / 答案缺失 / 幻觉选项）
const DEGRADED_NO_ANSWER: &str = "no_answer";

/// 浅答的轮次上限：恰一次模型调用（工具循环最多再跑一轮 dispatch）
pub(crate) const SHALLOW_MAX_ROUNDS: usize = 1;

/// 判据（prompt 存调用方，服务端不持有）
///
/// 只写黑名单侧：默认「深答」，只有确定一眼可答才浅答。三类**必须排除**在
/// 浅答面之外（宁可多跑几轮完整思考）：
/// ① 任何需要工具 / 查库 / 读文件 / 跑命令的诉求；
/// ② 多步任务的中间那句（「继续」「那就这么改」）—— 它短但不独立；
/// ③ 拿不准 —— 误判浅答的代价是用户只收到一句敷衍就结束。
const INSTRUCTIONS: &str = "请判断这条用户消息是否「一眼可答」：不需要查数据、不需要读文件、\
不需要跑工具、不需要多步推理，就能给出完整答复（典型：寒暄、致谢、简单事实确认、\
对上一轮结果的「好」「嗯」「收到」这类收尾确认）。\n\
只有确定是这类才选 shallow。其余一律选 deep。特别注意：\n\
① 只要需要调用工具、查库、读文件、跑命令，就选 deep；\n\
② 多步任务里的中间句（「继续」「那就这么改」）虽然短，但依赖上文与后续动作，选 deep；\n\
③ 拿不准就选 deep —— 误判 shallow 会把多步任务砍成一轮敷衍回复，代价远大于多跑几轮。";

/// 两个选项（id → 判据描述）
const OPTIONS: [(&str, &str); 2] = [
    (
        OPTION_DEEP,
        "需要完整思考：需要工具 / 检索 / 多步推理，或是多步任务的中间一句",
    ),
    (
        OPTION_SHALLOW,
        "一眼可答：寒暄、致谢、简单事实确认，或对上一轮结果的收尾确认，\
无需任何工具与多步推理",
    ),
];

/// 透传给模型的上下文事实：统一由 [`context::ContextBundle`] 渲染成单键分块正文
///
/// 键名收敛到 `context::STATE_KEY` 一处（单出口）：本文件不再出现
/// `agent_id` / `last_user_message` 这类裸键拼装。
const STATE_KEYS: &[&str] = &[context::STATE_KEY];

/// 浅答判决（零尺寸证据类型）
///
/// 刻意不用 `bool`：`Option<Shallow>` 让「没问出结论」「判深答」「判浅答」
/// 三种情形在类型上就分得开，调用点不可能把 `None` 误当 `Some(false)` 处理。
pub(crate) struct Shallow;

/// 本场景的策略形态（无状态，可反复构造）
fn policy() -> PickOnePolicy {
    PickOnePolicy::new(
        SCENE,
        "唤醒后浅答门",
        "这条用户消息是否一眼可答（不需要工具 / 检索 / 多步推理）",
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

/// **唯一翻译点**：判决 → 行为参数（轮次上限, 绝对强度）
///
/// 调用点只消费返回值，**不得自行翻译**（`nodes.rs` 里不出现字面量 `1` /
/// `Some(0)`：一处翻译，判决语义变了只改这里）。
///
/// 返回恰一个二元组、**不设第三出口** —— 多一个出口，调用点就又要分支解释，
/// 收口白做。`None` 分支原样透传 `effective_max_rounds` 且不注入强度，
/// 保证「小脑没配的组织」不会被误判成全员浅答。
pub(crate) fn apply(
    gate: Option<Shallow>,
    effective_max_rounds: usize,
) -> (usize, Option<ThinkingIntensity>) {
    match gate {
        Some(Shallow) => (SHALLOW_MAX_ROUNDS, Some(ThinkingIntensity::MIN)),
        None => (effective_max_rounds, None),
    }
}

/// 生产入口：判断本轮是否该浅答（`None` = 走原逻辑）
///
/// `total_rounds` 是 **run 级跨周期累计已消耗轮次**（`read_budget(bus).total_rounds`），
/// 不是周期内起始轮次。`brain` 刻意收 `Option<&Brain>`：调用方直接传
/// `agent.brain.as_ref()`，缺失即降级。
pub(crate) async fn maybe_shallow(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: Option<&Brain>,
    text: &str,
    trace_id: &str,
    total_rounds: usize,
) -> Option<Shallow> {
    // ⓪ 只在整次唤醒的第一轮问：续跑 / 溢出回环都在同一 run 里，再问一次
    //    只会把用户追加的后续输入也削成一轮。零调用、零配额、零轨迹 ——
    //    「没问」与「问了没结论」必须在观测上分得开。
    if total_rounds > 0 {
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

    // ② 未装配小脑 provider = 降级：**一次请求都不发**。
    //
    //    与 `think_loop` 里 `brain.cerebellum.is_some()` 的判据同源。这条同时是
    //    集成测试的护栏：`Brain::new_local` 的 `cerebellum` 恒为 `None`，
    //    故四路径集成用例不会摸到真实 DAL（不打网络，符合「集成测试不打模型」红线）。
    // 同 ①：写成 let-else/if 而不是 `brain.cerebellum.as_ref()?`，是要让降级点在调用处
    // 显式可见（`?` 在 Option 上下文只等价于提前 return None，不构成错误升级）。
    #[allow(clippy::question_mark)]
    if brain.cerebellum.is_none() {
        return None;
    }

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
            "wake_gate",
            decision = DECISION_FALLBACK,
            degraded_reason = reason.as_str(),
            scene = SCENE,
            latency_ms = latency_ms,
            "wake gate degraded to original logic"
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
            "wake_gate",
            decision = DECISION_FALLBACK,
            degraded_reason = DEGRADED_NO_ANSWER,
            scene = SCENE,
            latency_ms = latency_ms,
            "wake gate produced no usable answer"
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

    if pick.option == OPTION_SHALLOW {
        log_info!(
            ctx,
            "wake_gate",
            decision = DECISION_SHALLOW,
            confidence = pick.confidence,
            scene = SCENE,
            latency_ms = latency_ms,
            "awaken answered in shallow mode by cerebellum"
        );
        record_cerebellum(
            trace_id,
            trace(
                DECISION_SHALLOW,
                Some(pick.confidence),
                None,
                true,
                traced_latency_ms,
            ),
        );
        return Some(Shallow);
    }

    // 放行（deep）：下游行为与没有本门时一致，故 `applied=false`；留痕只为算
    // 浅答率（分母 = 本 scene 的轨迹条数，分子 = decision=shallow）。
    log_info!(
        ctx,
        "wake_gate",
        decision = DECISION_DEEP,
        confidence = pick.confidence,
        scene = SCENE,
        latency_ms = latency_ms,
        "wake gate lets original path run"
    );
    record_cerebellum(
        trace_id,
        trace(
            DECISION_DEEP,
            Some(pick.confidence),
            None,
            false,
            traced_latency_ms,
        ),
    );
    None
}

#[cfg(test)]
#[path = "wake_gate_tests.rs"]
mod tests;
