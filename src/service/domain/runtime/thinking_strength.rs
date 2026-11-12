//! 思考强度选择（小脑按 `score` 题直接给连续分）
//!
//! 2026-11-11 AMan 拍板：强度是 0–100 的**连续量**，不该用 choice 题型切档、
//! 再在客户端对 `probabilities` 加权补回连续值。`score` 题的服务端已经把各
//! 等级概率加权成一个可落在两级之间的分数，客户端只做一次线性缩放：
//!
//! ```text
//! strength = score_to_unit(raw, levels.len()) * UPPER
//! ```
//!
//! 于是**等级数**（协议允许 2~255）与**业务上限**（本场景 100）彻底解耦：
//! 等级只是给模型的语义锚点，输出不再被档位离散化。实测同一条
//! 「排查修复 Rust 模块」消息：11 级语义 `score` 题返回 6.95 → 69.5 分
//! （`confidence=0.79`）；旧 11 档 choice 只能取 0/10/…/100，还要靠客户端
//! 加权才得到 73 分。
//!
//! # 为什么不铺满协议的 255 级
//!
//! 255 是**允许的上限**，不是每次都该用的档数。实测 255 级纯数字刻度：
//! 概率摊薄到每级 0.01~0.02，服务端 `score=103.29` 与其自身
//! `probabilities` 手算期望 29.79 相差 73.5，归一后只有 40.67 分。
//! 等级越多、描述越没语义，模型判别力越差。所以这里保留 11 条**带语义**
//! 的锚点，靠比例缩放拿到 0–100 的任意整数，而不是人为造 101/255 个档位。
//!
//! # 降级（业务方忽略错误、走原逻辑）
//!
//! 无小脑 / 调用失败 / 超时 / 答案不可用 → `choice = None`，调用方退回
//! 原逻辑（Provider 基准 + Agent 覆盖 + 场景偏移 + 意图确定性偏移）。
//! 小脑是可选增强，绝不阻塞主流程，也不把「没判断出来」当成「判断为最低强度」。
//!
//! ⚠️⚠️ **不做单档置信门禁**：`score` 的 `confidence` 是打分置信度，只作观测。
//! 此前用 11 档 choice + 「top-1 `confidence >= 0.5`」当门禁，而连续量的
//! top-1 概率天然只有 0.18~0.25，导致强度决策 79 次全量降级、0 次选中
//! （生产日志 11-10~11-11）。换题型后这个门禁连同加权补丁一起删除。
//!
//! 返回 [`StrengthSelection`] 而不是 `Option`：调用方（思考轨迹）需要同时知道
//! **问没问**（`latency_ms`）与**为什么没成**（`degraded`）。几种成因若折叠成
//! 一个值，「配额耗尽」会被记成「答案不可用」，统计上无法归因。
//!
//! # Provider 未配置 `thinking_intensity` 时
//!
//! 分数照常算出并**写进思考轨迹**（`applied=false`），但请求体不下发该字段——
//! 能力开关的唯一判定点在 `think_round::runner::resolve_call_options`，这里
//! 不重复判。这样「算出了但没生效」是可观测的，不是静默失效。

use super::cerebellum_context as context;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::policy::{Metrics, Policy, ScorePolicy};
use crate::service::dal::brain::{BrainDal, dal};
use common::enums::{ThinkingIntensity, ThinkingScene};
use std::sync::Arc;

/// 场景标识（日志 / 计量口径，也是 qid 命名空间前缀）
pub const SCENE: &str = "thinking_strength";

/// 轨迹里的阶段标签：`thinking_strength.<phase>`（2026-11-10 硬约束）
///
/// 强度决策在 awaken / intent-analyze / summary / compact / settle **每条链路各调
/// 一次**，但它们是**同一个决策 scene**（同一策略、同一问题、同一 800ms 预算，
/// 只是 `state.scene` 不同）。所以：
/// - `DecisionRequest.scene` 恒为 [`SCENE`]（不拆 scene，qid 命名空间唯一）；
/// - 轨迹/日志用本函数给出阶段后缀，否则看板读不出是哪一次决策（Q9 缺口：
///   P2 补上 intent 后，awaken / summary / intent 会三合一）。
pub const fn trace_scene(thought: ThinkingScene) -> &'static str {
    match thought {
        ThinkingScene::Awaken => "thinking_strength.awaken",
        ThinkingScene::IntentAnalyze => "thinking_strength.intent",
        ThinkingScene::Summary => "thinking_strength.summary",
        ThinkingScene::Compact => "thinking_strength.compact",
        ThinkingScene::Settle => "thinking_strength.settle",
    }
}

/// 业务上限：强度对外恒定 0–100 分（[`ThinkingIntensity`] 的值域）
///
/// 与协议的 255 级上限无关：等级数由 [`LEVELS`] 决定，比例缩放把服务端
/// 的判断值映射到这个上限，改业务刻度不必动题面、也不必重新切档。
const UPPER: f32 = 100.0;

/// 判据（prompt 存调用方，服务端不持有）
const INSTRUCTIONS: &str = "请判断用户这条消息需要多深的思考强度，在下面由低到高的等级中给出判断。\
分数越高代表需要越多的推理与工具调用；越低代表越可以直接回答。\
只按「这条消息本身需要多深的处理」判断，不要按身份、历史表现或情绪判断。";

/// 11 条**语义锚点**（不是输出档位）
///
/// `score` 题的等级是 0-based 序号，服务端返回的分数可以落在两级之间，
/// 再经 [`UPPER`] 缩放得到 0–100 的任意整数；这里的「N 分」只是给模型
/// 定位用，不像旧 choice 题那样把输出锁死在 11 个离散值上。
/// 数量在协议 2~255 边界内，不写死在 `ScorePolicy` 里。
const LEVELS: [&str; 11] = [
    "0 分：寒暄 / 客套 / 纯格式确认，无需任何推理",
    "10 分：一句话事实或简单确认",
    "20 分：简单解释或单步操作指引",
    "30 分：需要少量检索或一步工具调用",
    "40 分：需要综合几条已知信息作答",
    "50 分：常规任务，需要若干轮推理与工具调用",
    "60 分：多步任务，需要计划与自我验证",
    "70 分：复杂任务，涉及代码 / 数据 / 多文件改动",
    "80 分：高难度任务，需要深入分析与反复验证",
    "90 分：跨模块 / 跨项目深链推理，错一步代价高",
    "100 分：最高强度，需要穷尽式排查与充分自证",
];

/// 透传给模型的上下文事实（与路由同口径：只带轻量上下文，不取全量记忆）
///
/// 键名收敛到 [`context::STATE_KEY`] 一处（单出口）。原 `scene` 键放的是 **phase**
/// （`ThinkingScene::as_str()`），收敛后 phase 作为 `render_for` 的 scene 参数
/// 进正文「当前场景」块——该维度信息不丢。
const STATE_KEYS: &[&str] = &[context::STATE_KEY];

/// 一次强度选择的结果
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrengthChoice {
    /// 小脑判定的强度（0–100 分，已按比例缩放）
    pub strength: ThinkingIntensity,
    /// 打分置信度（只作观测，不作门禁）
    pub confidence: f32,
}

/// 轨迹 `degraded` 值域：**每个值对应唯一触发条件**
///
/// 2026-11-10 修正：此前几种成因统一返回 `None`，`think_loop` 于是把
/// 「配额耗尽」也记成 `no_usable_answer`——统计上无法归因。
pub const DEGRADED_NO_USER_MESSAGE: &str = "no_user_message";
pub const DEGRADED_BUDGET: &str = "budget_exhausted";
pub const DEGRADED_EMPTY_REQUESTS: &str = "empty_requests";
pub const DEGRADED_NO_ANSWER: &str = "no_usable_answer";

/// 一次强度选择的完整产出：选了没有 + 耗了多久 + 为什么没选
///
/// 与 [`StrengthChoice`] 的分工：后者只表达「选到了什么」，本结构额外承载
/// **观测口径**——轨迹要回答「这次到底问没问、问出了什么」，
/// 把「没问」（配额 / 无用户消息）与「问了但没答案」记成同一个值就是失真。
#[derive(Debug, Clone)]
pub struct StrengthSelection {
    /// 选中项；`None` = 本次没有可用结论（原因见 `degraded`）
    pub choice: Option<StrengthChoice>,
    /// 轨迹口径的耗时；`None` = 没真正发出请求
    pub latency_ms: Option<u64>,
    /// 降级/未命中原因；`None` 且 `choice.is_some()` = 正常选中
    pub degraded: Option<&'static str>,
}

impl StrengthSelection {
    /// 零调用早退：没问，所以耗时恒 `None`
    fn not_called(degraded: &'static str) -> Self {
        Self {
            choice: None,
            latency_ms: None,
            degraded: Some(degraded),
        }
    }

    /// 调用过但没拿到可用结论：耗时取实测值
    fn unusable(latency_ms: Option<u64>, degraded: &'static str) -> Self {
        Self {
            choice: None,
            latency_ms,
            degraded: Some(degraded),
        }
    }
}

/// 本场景的策略形态（无状态，可反复构造）
///
/// `min_score = 0`：本场景不用 `judge`（命中与否由「有没有结论」决定，
/// 低分是合法判断而不是未命中），阈值参数在这里无意义。
fn policy() -> ScorePolicy {
    ScorePolicy::new(
        SCENE,
        "思考强度选择",
        "这条用户消息需要多深的思考强度（0–100 分）",
        INSTRUCTIONS,
        LEVELS,
        0.0,
    )
    .with_upper(UPPER)
    .with_state_keys(STATE_KEYS)
}

/// 生产入口：小脑判定本次思考强度（`None` = 保持原逻辑）
pub async fn select(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: &Brain,
    thought: ThinkingScene,
    last_user_message: Option<&str>,
) -> StrengthSelection {
    // 没有用户消息就没有可判断的输入：直接走原逻辑，零调用。
    let Some(user_text) = last_user_message else {
        return StrengthSelection::not_called(DEGRADED_NO_USER_MESSAGE);
    };

    // 阶段标签：同一个决策 scene，用 phase 区分是哪条链路的那一次
    let phase = thought.as_str();

    // 本 run 的调用配额（上限 = resolved 大脑轮次）：超限静默走原逻辑
    if !brain.cerebellum_quota.try_acquire() {
        log_info!(
            ctx,
            "cerebellum_quota",
            scene = SCENE,
            phase = phase,
            limit = brain.cerebellum_quota.limit(),
            used = brain.cerebellum_quota.used(),
            "cerebellum call budget exhausted, fall back to original logic"
        );
        return StrengthSelection::not_called(DEGRADED_BUDGET);
    }

    let policy = policy();
    // 单出口（笔④-2 笔 B）：phase 仍进正文「当前场景」块，维度信息不丢
    let metrics = Metrics::new().with(
        context::STATE_KEY,
        context::render_for(brain, phase, user_text),
    );

    // 阶段一：策略声明要问什么（纯函数；未命中就一次调用都不发）
    let requests = policy.prepare(&metrics);
    if requests.is_empty() {
        // 续-3：被丢弃的请求必须留痕。⚠️ 本分支在现配置下结构不可达
        // （`LEVELS` 是 11 项编译期常量，恒在协议 2~255 边界内）⇒ 本行日志
        // 只能由代码审查保证，不计入「已测」。
        log_info!(
            ctx,
            "thinking_strength",
            decision = "empty_requests",
            scene = SCENE,
            phase = phase,
            degraded_reason = DEGRADED_EMPTY_REQUESTS,
            "thinking strength asked nothing, keep original logic"
        );
        return StrengthSelection::not_called(DEGRADED_EMPTY_REQUESTS);
    }

    // 阶段二：唯一 async 点（门禁 / 超时 / 协议转换 / 降级全在 DAL）
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;
    // 轨迹口径：`None` = 本次没真正发出请求（空请求门 / 三个零调用门）
    let traced_latency_ms = outcome.traced_latency_ms();

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "thinking_strength",
            decision = "fallback",
            degraded_reason = reason.as_str(),
            scene = SCENE,
            phase = phase,
            latency_ms = latency_ms,
            "thinking strength degraded to original logic"
        );
        return StrengthSelection::unusable(traced_latency_ms, reason.as_str());
    }

    // 阶段三：答案注入事实桶 → 服务端判断值按等级数归一，再乘业务上限。
    // 加权在服务端已完成，这里**只有一次线性缩放**；`confidence` 只作观测。
    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let Some(grade) = policy.scaled(&metrics) else {
        log_info!(
            ctx,
            "thinking_strength",
            decision = "no_answer",
            scene = SCENE,
            phase = phase,
            latency_ms = latency_ms,
            "thinking strength produced no usable answer"
        );
        return StrengthSelection::unusable(traced_latency_ms, DEGRADED_NO_ANSWER);
    };

    let choice = StrengthChoice {
        strength: ThinkingIntensity::new(grade.score.round().clamp(0.0, UPPER) as u8),
        confidence: grade.confidence,
    };

    log_info!(
        ctx,
        "thinking_strength",
        decision = "select",
        score = choice.strength.score(),
        confidence = choice.confidence,
        levels = LEVELS.len(),
        upper = UPPER,
        scene = SCENE,
        phase = phase,
        latency_ms = latency_ms,
        input_tokens = outcome.usage.input_tokens,
        output_tokens = outcome.usage.output_tokens,
        "thinking strength selected"
    );
    StrengthSelection {
        choice: Some(choice),
        latency_ms: traced_latency_ms,
        degraded: None,
    }
}

/// 单例 DAL 的生产封装
pub async fn select_with_default_dao(
    ctx: &RequestContext,
    brain: &Brain,
    thought: ThinkingScene,
    last_user_message: Option<&str>,
) -> StrengthSelection {
    select(ctx, dal(), brain, thought, last_user_message).await
}

#[cfg(test)]
#[path = "thinking_strength_tests.rs"]
mod tests;
