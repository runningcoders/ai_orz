//! 小脑路由器（B3 运行时快判断路由）
//!
//! 在 `run_think_loop` 首轮前做亚秒级小脑（System One decision 模型）快判断。
//! **两个决策点串行**（K1），各自独立一次请求、一条轨迹、一笔配额：
//!
//! 1. `entry_gate`：这个消息值不值得进主循环。判据**只写黑名单侧**——只有三类算
//!    「无价值」（在线性询问 / 连通性测试 / 纯标点或空串），其余一律「有价值」；
//! 2. `entry_pool`：**只**在门判「无价值」时才问——从池里选一句回复（K8）。
//!    池是模块内常量、选项 id 用 `pool_N`；选失败 / 配额耗尽 → 回退池内最中性一条。
//!
//! 路由层不向 Prompt 注入任何内容（K3 删 Inject）。
//!
//! 三层降级全部收敛为「静默跳过 = 现状」：
//!
//! 1. 无启用小脑（`brain.cerebellum = None`）→ 全域静默跳过，
//!    同时即全局回滚开关（管理面停用默认小脑 = 一键回滚，零代码零重启）；
//! 2. 调用失败 / 超时（`provider.config.timeout_ms`，缺省 1000ms）→ 静默跳过；
//! 3. 低置信 → 不直回（按「有价值」走原逻辑）。
//!
//! ⚠️ **每个决策点都必须过 `CerebellumQuota::try_acquire`**（同一本 per-run 账）：
//! 漏掉任何一个，配额就只护住旧点、新场景全在闸门外，等于没配。
//!
//! 三铁律口径：state 只携带轻量上下文（消息文本 + Agent 标识），不取全量记忆；
//! 单次调用受亚秒预算约束。
//!
//! 「问什么」不在这里写死：两个场景都用 [`PickOnePolicy`] 形态声明一次单选，
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

/// 入口门置信阈值：低于该值按「没判出来」处理（静默走原逻辑）
///
/// **独立阈值，不与其它场景共用**（旧路由的 0.85 随判据重写作废）：入口门判成
/// 「无价值」= 用户拿到的是一句池内回复，误判代价远高于多跑一次主循环，故取 0.9。
pub const ENTRY_GATE_CONFIDENCE_THRESHOLD: f32 = 0.9;

/// 入口门场景名（日志/计量口径，也是 qid 命名空间前缀）
pub const ENTRY_GATE_SCENE: &str = "entry_gate";

/// 「第 0 案」记账 scene：小脑**装配为空**时的留痕归类
///
/// 为什么单独开一个 scene 而不复用 `entry_gate` / `degraded`：装配缺失不是
/// 「某场景降级」，而是「本 run 根本没有小脑」这一上层事实。混进任何业务场景
/// 都会把该场景的降级率算脏（分母里塞进一批根本没调用过的样本）。
pub const BOOTSTRAP_SCENE: &str = "bootstrap";

/// 「第 0 案」记账结论：`brain.cerebellum == None`（未装配/未启用小脑模型）
///
/// 与「装配了但本场景未触发」严格区分：后者在任何 scene 下都不会出现本条，
/// 前者必然出现 —— 这正是本条留痕要回答的唯一问题。
pub const BOOTSTRAP_NOT_ASSEMBLED: &str = "not_assembled";

/// 入口候选池场景名
pub const ENTRY_POOL_SCENE: &str = "entry_pool";

/// 入口池置信阈值：与门同档（0.9），但**独立成常量**——两侧失败语义不同
/// （门失败 = 走原逻辑；池失败 = 回退最中性一条），将来单独调参时不动对方。
pub const ENTRY_POOL_CONFIDENCE_THRESHOLD: f32 = 0.9;

/// 池侧回退的留痕口径（`degraded` 字段用；细因另记日志）
pub const POOL_FALLBACK_DEGRADED: &str = "pool_fallback";

/// 两个选项
pub const CHOICE_TRIVIAL_DIRECT: &str = "trivial_direct";
pub const CHOICE_CORTEX_NEEDED: &str = "cortex_needed";

/// 入口门判据（prompt 存调用方，服务端不持有）
///
/// **只写黑名单侧**：只有明确落入三类「无价值」才判无价值，其余默认有价值——
/// 判据漏写/模型保守时，最坏结果是多跑一次主循环，而不是把正常消息吞成池内回复。
const ENTRY_GATE_INSTRUCTIONS: &str = "判断用户消息是否属于『无价值』，只有三类算无价值：\
①在线性询问（在吗 / 在不在）；②连通性测试（测试一下 / 1+1 这类探针）；③纯标点或空串。\
其余一律有价值：寒暄、客套、致谢、简单问题、单个名词，以及任何具体任务，都必须当作有价值。";

/// 入口池判据：只决定「用哪一句」，**不重新判定消息价值**（那是门的事）
const ENTRY_POOL_INSTRUCTIONS: &str = "从候选回复里挑一句最适合回应当前用户消息的：\
表示在场/在线的，用于『在吗』这类询问；连通性测试、纯标点或空串，挑最短最中性的一句。\
候选都只是简短承接语，不要挑与消息场景不符的。";

/// 透传给模型的上下文事实（三铁律：只带轻量上下文，不取全量记忆）
const ENTRY_STATE_KEYS: &[&str] = &["agent_id", "agent_name", "last_user_message"];

/// 入口候选池 id（顺序即语义序，`pool_0` = 最中性）
///
/// id **不用文案本身**：文案会重写，id 才是轨迹与计量的稳定锚点。
pub const ENTRY_POOL_IDS: [&str; 4] = ["pool_0", "pool_1", "pool_2", "pool_3"];

/// 入口候选池文案（2026-11-10 起取代原单条 `TRIVIAL_DIRECT_TEMPLATE`）
///
/// 三条硬约束：①无假陈述（不声称消息简单 / 不声称已完成某事）②无自指矛盾
/// ③不承诺具体能力（常量全体 Agent 共用，职业中立）。`ENTRY_POOL[0]` 是最中性的
/// 一条，选失败 / 配额耗尽一律回退它。
pub const ENTRY_POOL: [&str; 4] = [
    "收到。",
    "我在，你说。",
    "收到，有什么要处理的直接说。",
    "在的。需要我做什么，直接讲。",
];

// ==================== 决策类型 ====================

/// 入口池结论
#[derive(Debug, Clone, PartialEq)]
pub enum PoolReply {
    /// 选出来了：`id` = `pool_N`
    Picked {
        /// 池内 id（`pool_N`），不是文案
        id: String,
        /// 选中概率
        confidence: f32,
    },
    /// 没选出来：回退池内最中性一条（`ENTRY_POOL[0]`）
    Fallback {
        /// 降级原因（日志/轨迹细因）
        reason: DegradedReason,
    },
}

/// 路由决策三分类
#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// 无价值：候选池直回（池内一条）
    Direct {
        /// 回复文案
        content: String,
        /// 入口门置信度
        confidence: f32,
        /// 池侧结论（选出来了 / 回退）
        pool: PoolReply,
    },
    /// 有价值：不做任何干预，按原逻辑进入主循环
    ///
    /// 与 `Skip` 的差别是观测口径：`Proceed` = 判出来了且判为有价值，`Skip` =
    /// 压根没判出来（降级）。删掉 Inject 后，路由层不再向 Prompt 注入任何内容。
    Proceed { confidence: f32 },
    /// 静默跳过 = 现状（降级原因用于 B5 观测）
    Skip { reason: DegradedReason },
}

/// 入口门结论（内部类型：直回还要再问一次池，故与对外的 [`RouteDecision`] 分开）
#[derive(Debug, Clone, PartialEq)]
enum GateVerdict {
    Direct { confidence: f32 },
    Proceed { confidence: f32 },
    Skip { reason: DegradedReason },
}

// ==================== 入口 ====================

/// 入口门形态：一次单选，选项固定
///
/// 三铁律之一（只带轻量上下文）体现为 `with_state_keys`：`Metrics` 里可以有很多事实，
/// 但透传给小脑的只有声明过的那几项。
fn entry_gate_policy() -> PickOnePolicy {
    PickOnePolicy::new(
        ENTRY_GATE_SCENE,
        "小脑入口门",
        "首轮快速判断是否需要完整思考",
        ENTRY_GATE_INSTRUCTIONS,
        [
            (
                CHOICE_TRIVIAL_DIRECT,
                "无价值（只有三类）：在线性询问（在吗 / 在不在）、连通性测试（测试一下 / 1+1 这类探针）、纯标点或空串",
            ),
            (
                CHOICE_CORTEX_NEEDED,
                "有价值（默认）：寒暄、客套、致谢、简单问题、单个名词，以及任何具体任务——都必须进入主循环正常回复",
            ),
        ],
        ENTRY_GATE_CONFIDENCE_THRESHOLD,
    )
    .with_state_keys(ENTRY_STATE_KEYS)
}

/// 入口池形态：一次单选，选项 = 池内文案（id 为 `pool_N`）
fn pool_policy() -> PickOnePolicy {
    PickOnePolicy::new(
        ENTRY_POOL_SCENE,
        "小脑入口候选池",
        "为被判无价值的消息选一句承接回复",
        ENTRY_POOL_INSTRUCTIONS,
        ENTRY_POOL_IDS
            .iter()
            .zip(ENTRY_POOL.iter())
            .map(|(id, text)| (*id, *text)),
        ENTRY_POOL_CONFIDENCE_THRESHOLD,
    )
    .with_state_keys(ENTRY_STATE_KEYS)
}

/// 一次路由的完整产出：结论 + **两条轨迹各自的耗时**
///
/// 门与池是两次独立的模型调用（`entry_gate` / `entry_pool`），所以必须两列。
/// 合成一个数字的话，「门没调用、池花了 7ms」与「门花了 7ms、池没调用」
/// 在轨迹上会完全一样——那正是本次要修掉的观测失真。
#[derive(Debug, Clone)]
pub struct RouteOutcome {
    /// 路由结论（三分类）
    pub decision: RouteDecision,
    /// `entry_gate` 那一次调用的轨迹口径耗时；`None` = 没发出请求
    pub gate_latency_ms: Option<u64>,
    /// `entry_pool` 那一次调用的轨迹口径耗时；`None` = 门没判 Direct / 池没调用
    pub pool_latency_ms: Option<u64>,
}

impl RouteOutcome {
    /// 零调用早退（本地可判 / 配额闸）：两笔耗时都是 `None`
    fn not_called(reason: DegradedReason) -> Self {
        Self {
            decision: RouteDecision::Skip { reason },
            gate_latency_ms: None,
            pool_latency_ms: None,
        }
    }
}

/// B3 运行时快判断路由：`entry_gate` → （仅当判无价值）`entry_pool`
///
/// - `dal` 参数化 trait 对象：生产 = `dal::brain::dal()`（BrainDal::decide），测试注入 mock；
/// - provider 未装配 / 未配好 → 由 DAL 返回 `NoProvider`，本层不判空。
///   ⚠️ 生产路径 `think_loop` 外面套了 `brain.cerebellum.is_some()`，所以现场
///   只可能看到「已装配但 api_key 空」这一种 `NoProvider`；「未装配」根本进不来。
/// - state 只携带轻量上下文（last_user_message + agent 标识），不取全量记忆；
/// - 超时 / 协议转换 / 降级全在 DAL，任何失败 → Skip（= 现状）。
pub async fn route(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: &Brain,
    last_user_message: Option<&str>,
) -> RouteOutcome {
    let Some(user_text) = last_user_message else {
        return RouteOutcome::not_called(DegradedReason::NoUserMessage);
    };

    // 本 run 的调用配额（上限 = resolved 大脑轮次）：超限静默走原逻辑
    if !brain.cerebellum_quota.try_acquire() {
        log_info!(
            ctx,
            "cerebellum_quota",
            scene = ENTRY_GATE_SCENE,
            limit = brain.cerebellum_quota.limit(),
            used = brain.cerebellum_quota.used(),
            "cerebellum call budget exhausted, fall back to original logic"
        );
        return RouteOutcome::not_called(DegradedReason::BudgetExhausted);
    }

    // 轻量上下文（三铁律：不取全量记忆、先于小脑的仅有消息文本）
    let policy = entry_gate_policy();
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
    // 轨迹口径：`None` = 本次没真正发出请求（空请求门 / 三个零调用门）
    let traced_latency_ms = outcome.traced_latency_ms();

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "cerebellum_route",
            decision = "skip",
            degraded_reason = reason.as_str(),
            latency_ms = latency_ms,
            "cerebellum entry gate degraded"
        );
        return RouteOutcome {
            decision: RouteDecision::Skip { reason },
            gate_latency_ms: traced_latency_ms,
            pool_latency_ms: None,
        };
    }

    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let verdict = translate_gate(&policy, &metrics);

    // K1：门判「无价值」后才串第二次决策（用哪一句）——两次请求、两条轨迹、两笔配额。
    // 池侧任何失败都**不会**退回主循环：门已经放行了直回，此处只会回退池内最中性一条。
    let mut pool_latency_ms = None;
    let decision = match verdict {
        GateVerdict::Skip { reason } => RouteDecision::Skip { reason },
        GateVerdict::Proceed { confidence } => RouteDecision::Proceed { confidence },
        GateVerdict::Direct { confidence } => {
            let (content, pool, pool_ms) = select_pool_reply(ctx, &dal, brain, user_text).await;
            pool_latency_ms = pool_ms;
            RouteDecision::Direct {
                content,
                confidence,
                pool,
            }
        }
    };

    match &decision {
        RouteDecision::Direct {
            content,
            confidence,
            pool,
        } => {
            let (pool_id, pool_degraded) = match pool {
                PoolReply::Picked { id, .. } => (id.as_str(), "none"),
                PoolReply::Fallback { reason } => ("fallback", reason.as_str()),
            };
            log_info!(
                ctx,
                "cerebellum_route",
                decision = "direct",
                confidence = confidence,
                pool = pool_id,
                pool_degraded = pool_degraded,
                latency_ms = latency_ms,
                input_tokens = outcome.usage.input_tokens,
                output_tokens = outcome.usage.output_tokens,
                content_chars = content.len(),
                "cerebellum entry gate decision"
            );
        }
        RouteDecision::Proceed { confidence } => {
            log_info!(
                ctx,
                "cerebellum_route",
                decision = "proceed",
                confidence = confidence,
                latency_ms = latency_ms,
                input_tokens = outcome.usage.input_tokens,
                output_tokens = outcome.usage.output_tokens,
                "cerebellum entry gate decision"
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
                "cerebellum entry gate decision"
            );
        }
    }
    RouteOutcome {
        decision,
        gate_latency_ms: traced_latency_ms,
        pool_latency_ms,
    }
}

/// 入口候选池：给「已被门判为无价值」的消息选一句回复（K8）
///
/// 返回 `(content, pool)`：门已经放行了直回，所以**任何失败路径都必须给出一句回复**，
/// 唯一合法的回退是池内最中性一条（`ENTRY_POOL[0]`），并记 `Fallback{reason}`。
async fn select_pool_reply(
    ctx: &RequestContext,
    dal: &Arc<dyn BrainDal>,
    brain: &Brain,
    user_text: &str,
) -> (String, PoolReply, Option<u64>) {
    // 配额：与入口门同一本 per-run 账（新增决策点同样必须过闸，否则配额形同虚设）
    if !brain.cerebellum_quota.try_acquire() {
        log_info!(
            ctx,
            "cerebellum_quota",
            scene = ENTRY_POOL_SCENE,
            limit = brain.cerebellum_quota.limit(),
            used = brain.cerebellum_quota.used(),
            "cerebellum call budget exhausted, fall back to original logic"
        );
        return (
            ENTRY_POOL[0].to_string(),
            PoolReply::Fallback {
                reason: DegradedReason::BudgetExhausted,
            },
            None,
        );
    }

    let policy = pool_policy();
    let metrics = Metrics::new()
        .with("agent_id", &brain.agent_id)
        .with("agent_name", &brain.agent_name)
        .with("last_user_message", user_text);

    let requests = policy.prepare(&metrics);
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;
    // 轨迹口径：`None` = 本次没真正发出请求
    let traced_latency_ms = outcome.traced_latency_ms();

    let fallback = |reason: DegradedReason| {
        log_info!(
            ctx,
            "cerebellum_pool",
            decision = "fallback",
            degraded_reason = reason.as_str(),
            latency_ms = latency_ms,
            "cerebellum pool degraded, fall back to most neutral reply"
        );
        (
            ENTRY_POOL[0].to_string(),
            PoolReply::Fallback { reason },
            traced_latency_ms,
        )
    };

    if let Some(reason) = outcome.degraded {
        return fallback(reason);
    }

    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let Some(PickOne { option, confidence }) =
        policy.pick_one(&metrics, ENTRY_POOL_CONFIDENCE_THRESHOLD)
    else {
        // 区分「答了但置信不足」与「没答 / 形态不符」——观测口径不同，处理相同（都回退）
        let reason = match metrics.decision(&policy.primary_qid()) {
            Some(DecisionValue::Pick { .. }) => DegradedReason::LowConfidence,
            _ => DegradedReason::UnsupportedAnswer,
        };
        return fallback(reason);
    };

    // 模型可能编造池外 id：不在池内 = 没有结论（绝不拿它去查文案）
    let Some(idx) = ENTRY_POOL_IDS.iter().position(|id| *id == option.as_str()) else {
        return fallback(DegradedReason::UnsupportedAnswer);
    };

    log_info!(
        ctx,
        "cerebellum_pool",
        decision = option.as_str(),
        confidence = confidence,
        latency_ms = latency_ms,
        input_tokens = outcome.usage.input_tokens,
        output_tokens = outcome.usage.output_tokens,
        "cerebellum pool decision"
    );
    (
        ENTRY_POOL[idx].to_string(),
        PoolReply::Picked {
            id: option,
            confidence,
        },
        traced_latency_ms,
    )
}

/// 门结论 → 三分类（纯函数，单测覆盖三分类）
///
/// 规则：cortex_needed 且置信达标 → Proceed（零干预）；trivial_direct 且置信达标
/// → Direct（还要再问池）；其余（无结论 / 非 Pick / 未知选项 / 低置信）→ Skip。
///
/// ⚠️ `None`（没问出来）必须走 Skip，绝不能落到 Direct/Proceed ——
/// 「降级 = 未命中」，把「没判断出来」当成「判断为琐碎」会静默改变主流程。
fn translate_gate(policy: &PickOnePolicy, metrics: &Metrics) -> GateVerdict {
    let Some(PickOne { option, confidence }) =
        policy.pick_one(metrics, ENTRY_GATE_CONFIDENCE_THRESHOLD)
    else {
        // 区分「答了但不可用」的两种口径：Pick 但置信不足 → 低置信；
        // 其它（没答 / 题型不符 / 模型编造）→ 形态不符。
        let reason = match metrics.decision(&policy.primary_qid()) {
            Some(DecisionValue::Pick { .. }) => DegradedReason::LowConfidence,
            _ => DegradedReason::UnsupportedAnswer,
        };
        return GateVerdict::Skip { reason };
    };
    match option.as_str() {
        // 判为有价值 = 什么都不做（K3：删 Inject，路由层不再注入增强 System）
        CHOICE_CORTEX_NEEDED => GateVerdict::Proceed { confidence },
        CHOICE_TRIVIAL_DIRECT => GateVerdict::Direct { confidence },
        _ => GateVerdict::Skip {
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
    last_user_message: Option<&str>,
) -> RouteOutcome {
    route(
        ctx,
        crate::service::dal::brain::dal(),
        brain,
        last_user_message,
    )
    .await
}

#[cfg(test)]
#[path = "cerebellum_router_tests.rs"]
mod tests;
