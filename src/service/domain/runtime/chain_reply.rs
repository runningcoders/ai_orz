//! 对话链深收敛的小脑判据（P2-A · scene = `chain_reply`）
//!
//! # 为什么需要它
//!
//! 防 A↔B 无限乒乓原先只有一条静态判据（`pkg::collaboration_pressure`：疲劳度 +
//! 连续无新信息窗口），到阈值就 `CloseChainAsNotify`。静态判据**只看跳数不看正文**，
//! 于是两种误判同时存在：
//! - 链上明明还在产出新结论（工具结果、新参与方回血），却被硬墙封掉；
//! - 链上早已只剩「收到 / 已完成」的复述，却要白白再走几跳才收口。
//!
//! 本场景把「这一跳还有没有信息量、还需不需要回发」交给小脑判一次内容，
//! **静态阈值完整保留为兜底**：小脑没配 / 失败 / 超时 / 判不出 → 一律落回原逻辑。
//!
//! # 落点与调用顺序（由 `consumer::message` 保证）
//!
//! ```text
//! 静态规则未命中（= 跨 Agent 对等候选）
//!   → collect_chain_pressure 得 pressure（+ 顺手收链内正文，零额外 DB 读）
//!   → 达到触发阈值 且 小脑可用：本模块判一次
//!        close    → CloseChainAsNotify（复用现成收口）
//!        continue → 落到下一步
//!        降级/空输入/未触发 → 落到下一步
//!   → judge_chain_reply_route(pressure)  ← 原样，逐字节不变
//! ```
//!
//! **判据只用一次 `PickOne`**，候选两条（`continue` / `close`），不逐候选各发一题。
//!
//! # 触发阈值（两条件并列，缺一会漏判）
//!
//! - `fatigue >= soft_fatigue`：纯文本往返累积到压力带；
//! - `window_depth >= hard_window_depth - 1`：**回血**（工具结果 / 新参与方）会把
//!   疲劳度扣回去、窗口清零，让 `fatigue` 长期到不了软阈值；但窗口一旦撞满
//!   `hard_window_depth` 就直接 `Escalate` 封链 —— 这些链会「一次小脑都没问过
//!   就被封」。并列条件让它们在封链前一跳被问到。
//!
//! # 输入侧（Q3 定谳：条数 32 / token 预算 800 / 双阈取小）
//!
//! 链内消息由调用方沿 `reply_to_id` 回溯时**顺手收集**（那一趟本来就要拿每条
//! 消息做 `ChainHop` 分类，正文已在手里，零额外读库）。
//!
//! 预算分配是**有界且显式**的：总输入预算 [`INPUT_TOKEN_BUDGET`] = 800 tokens，
//! 其中 [`FRAME_OVERHEAD_RESERVE`] = 400 预留给判据骨架（instructions + options +
//! 压力事实 + 协议包装），余下 [`body_token_budget`] = 400 给链内正文。
//! 800 是全条 prompt 的上界——只给正文留 800 会让总输入贴住 1000ms 硬顶。
//!
//! 超长降级：**保链头 1 条 + 最近 N 条**，中间被丢弃的部分必须在 prompt 里
//! 显式写出「中间 N 条已省略（共 M 条）」。⚠️ 这一行是本场景唯一的假阴性形态：
//! 不标注，小脑会把「看不到」当成「没有新信息」→ 判 close → 后半段被截掉的
//! 链被误封。判据里因此写死「证据不足就选 continue」。
//!
//! # 降级不变量（三条，与《小脑快判断的运行口径》一致）
//!
//! 1. `None` 必走原逻辑：本模块**绝不产出业务结论**，只产出「要不要收口」的建议；
//! 2. 三态严格分：`degraded`（没判出来）/ `applied=false`（判了但未改变下游，
//!    含 `continue` 与 `not_triggered`）/ 正常（`close` 且生效）；
//! 3. 收口类决策必须自补下游最小输入：`close` 走的是**现成**收口路径
//!    （`CloseChainAsNotify`：Final 仍作为知会送达对方 + 通知归属用户），
//!    因此「消息必达」语义不变、收口路径仍可用。
//!
//! # 观测
//!
//! 每次进入本门都写一条 `record_cerebellum{scene: "chain_reply", decision}`。
//! `decision` 五值 = `not_triggered` / `empty_input` / `continue` / `close` /
//! `fallback`。**记账条目（not_triggered / empty_input / continue）同样占轨迹名额**
//! —— 一次唤醒最多 1 条（一次回发判定一次），单 run 32 条上限内绰绰有余。

use super::compaction::estimate_tokens;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::policy::{Metrics, PickOnePolicy, Policy};
use crate::pkg::think_trace::{ThinkTraceCerebellum, record_cerebellum};
use crate::service::dal::brain::{BrainDal, dal};
use std::sync::Arc;

/// 场景标识（日志 / 计量口径，也是 `DecisionRequest.scene`）
pub(crate) const SCENE: &str = "chain_reply";

/// 置信阈值：低于此值按「没结论」处理
///
/// 取 0.9（高于路由 0.85）：两侧代价不对称 —— 错判 `close` 会让一条仍在产出
/// 新结论的链被收口（对方只能自己重开新链），错判 `continue` 只是多走一跳，
/// 而静态阈值仍会在硬墙处兜底。故收口必须高置信。
pub(crate) const MIN_CONFIDENCE: f32 = 0.9;

/// 选项 id：继续自动回发
pub(crate) const OPTION_CONTINUE: &str = "continue";
/// 选项 id：收口（不再自动回发，改知会 + 通知归属用户）
pub(crate) const OPTION_CLOSE: &str = "close";

/// 轨迹决策值：未达触发阈值（本门不适用，零调用零配额）
pub(crate) const DECISION_NOT_TRIGGERED: &str = "not_triggered";
/// 轨迹决策值：链内一条消息都拉不到（零调用零配额）
pub(crate) const DECISION_EMPTY_INPUT: &str = "empty_input";
/// 轨迹决策值：模型判继续回发（下游与无本门时一致，`applied=false`）
pub(crate) const DECISION_CONTINUE: &str = "continue";
/// 轨迹决策值：模型判收口（改变下游路由，`applied=true`）
pub(crate) const DECISION_CLOSE: &str = "close";
/// 轨迹决策值：降级（`degraded` 给出原因）
pub(crate) const DECISION_FALLBACK: &str = "fallback";

/// 降级原因：本 run 小脑调用配额耗尽
const DEGRADED_BUDGET: &str = "budget_exhausted";
/// 降级原因：判了但没有可用结论（低置信 / 答案缺失 / 幻觉选项）
const DEGRADED_NO_ANSWER: &str = "no_answer";

/// 本场景输入的 token 总预算（Q3 定谳：800，不是 1000）
///
/// 否决 1000 的理由：1000 tokens ≈ 1.0s 已贴住 1000ms 的硬顶，按 1000 定预算
/// 等于「设计上就注定超时」，功能会恒定降级、退回静态阈值，这笔就白做了。
pub(crate) const INPUT_TOKEN_BUDGET: usize = 800;

/// 判据骨架的固定开销预留（instructions + options + 压力事实 + 协议包装）
///
/// 实测口径见 P3 联合核算：预留给多不留给少 —— 宁可少带两条链内消息，
/// 也不让总输入越过 [`INPUT_TOKEN_BUDGET`]。
pub(crate) const FRAME_OVERHEAD_RESERVE: usize = 400;

/// 链内消息条数上限（与 `consumer::message::MAX_CHAIN_TRAVERSAL` 同口径）
///
/// 与 [`body_token_budget`] 是**取小**关系：先按条数裁窗口（保链头 + 最近
/// `MAX_CHAIN_MESSAGES - 1` 条），再按 token 预算裁尾窗口。
pub(crate) const MAX_CHAIN_MESSAGES: usize = 32;

/// 链内正文可用的 token 预算 = 总预算 − 骨架预留
pub(crate) fn body_token_budget() -> usize {
    INPUT_TOKEN_BUDGET.saturating_sub(FRAME_OVERHEAD_RESERVE)
}

/// 判据（prompt 存调用方，服务端不持有）
///
/// 只写黑名单侧（默认 continue）：误收口的代价高于多回一轮。
/// 末段「省略标注 ⇒ 证据不足选 continue」是本场景唯一的假阴性防线。
const INSTRUCTIONS: &str = "请判断「协作方刚发来的这条消息」还有没有新的实质信息，\
本轮是否还需要自动回发。\n\
判断只看链内消息本身：这条新消息有没有带来新事实、新结论、新问题、新的待办或新的数据；\
不要因为「对方在等」或「出于礼貌」就选 continue。\n\
选 close（不再自动回发）：\n\
① 纯确认 / 致谢 / 收讫 / 复述：内容只是复述上文，或只是说「收到」「明白」「已完成」「好的」；\n\
② 结论已交付且没有新问题：对方要的答案已经在上文给出，本条只是重复或总结；\n\
③ 双方在同一层意思上来回换措辞，没有新增事实或新决策点。\n\
选 continue（继续回发）：\n\
① 本条引入了上文没有的新事实、新数据、新结论、新问题、新待办；\n\
② 本条提出了需要答复的具体问题、需要确认的决策点、需要执行的下一步；\n\
③ 判不准就选 continue —— 误收口会让协作中断，代价高于多回一轮。\n\
注意：链内消息区块若出现「中间 N 条已省略」，说明你看到的不是全链。\
不得把「看不到」当成「没有新信息」；证据不足一律选 continue。";

/// 两个选项（id → 判据描述）
const OPTIONS: [(&str, &str); 2] = [
    (
        OPTION_CONTINUE,
        "继续回发：本条带来了上文没有的新事实 / 新结论 / 新问题 / 新待办，\
或链内消息被截断导致证据不足、判不准",
    ),
    (
        OPTION_CLOSE,
        "不再自动回发：本条只是确认 / 致谢 / 收讫 / 复述，或同一层意思反复换措辞，\
没有新增事实或新决策点",
    ),
];

/// 透传给模型的上下文事实（轻量：只有压力数字与链内正文，不取记忆 / 工具）
const STATE_KEYS: &[&str] = &[
    "agent_id",
    "agent_name",
    "chain_depth",
    "window_depth",
    "fatigue",
    "soft_fatigue",
    "hard_fatigue",
    "hard_window_depth",
    "chain_messages",
];

/// 链内一条消息（小脑判据的最小输入）
///
/// 刻意**不持有领域类型**：只留「谁说的 + 什么类型 + 正文」，与
/// `collaboration_pressure` 同口径（组件不认识 `Message`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChainMessage {
    /// 发送方 ID（当前协作的两方之一）
    pub from_id: String,
    /// 消息类型的中文短标签（文本消息 / 工具执行结果 / …）
    pub type_label: &'static str,
    /// 正文（调用方原样给，本模块负责按预算截断）
    pub content: String,
    /// 创建时间戳（毫秒）；不参与渲染，仅供调用方/日志排序核对
    pub created_at: i64,
}

/// 协作压力事实（本模块只读数字，不依赖 `pkg::collaboration_pressure` 类型）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChainPressureFacts {
    pub chain_depth: usize,
    pub window_depth: usize,
    pub fatigue: u64,
    pub soft_fatigue: u64,
    pub hard_fatigue: u64,
    pub hard_window_depth: u64,
}

impl ChainPressureFacts {
    /// 是否达到「接近静态上限」的提问阈值（两条件**并列**）
    ///
    /// 只有 `fatigue` 会漏掉「靠回血续命的链」：`ToolResult` / `NewParticipant`
    /// 会把疲劳度扣回去、窗口清零，疲劳度永远涨不到软阈值，而窗口一撞满
    /// `hard_window_depth` 就直接封链 —— 这些链会一次都没被小脑看过。
    pub(crate) fn should_ask(&self) -> bool {
        self.fatigue >= self.soft_fatigue
            || (self.window_depth as u64) >= self.hard_window_depth.saturating_sub(1)
    }
}

/// 组装后的链内输入
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChainInput {
    /// 渲染好的区块（含省略标注；空串表示一条都没带上）
    pub rendered: String,
    /// 实际带上的消息条数（不含省略标注行）
    pub included: usize,
    /// 被省略的中间消息条数
    pub omitted: usize,
}

/// 省略标注行（Q3 硬要求：必须显式写出「中间 N 条已省略（共 M 条）」）
pub(crate) fn omission_line(omitted: usize, total: usize) -> String {
    format!("[中间 {omitted} 条已省略（共 {total} 条链内消息；仅保留链头与最近若干条）]")
}

/// 渲染单条链内消息
pub(crate) fn render_one(index: usize, message: &ChainMessage) -> String {
    format!(
        "[#{}] {} {}: {}",
        index + 1,
        message.from_id,
        message.type_label,
        message.content.trim()
    )
}

/// 按 token 预算截断到最大可行前缀，并留下截断标注
///
/// 标注本身的 token 也计入 `budget`：否则「正好截到预算」再拼一段标注就会
/// 越界（实测过 436 > 400）。返回值恒满足 `estimate_tokens(..) <= budget`。
fn truncate_to_tokens(text: &str, budget: usize) -> String {
    if estimate_tokens(text) <= budget {
        return text.to_string();
    }
    const SUFFIX: &str = "…（正文已截断）";
    let body_budget = budget.saturating_sub(estimate_tokens(SUFFIX));
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let prefix: String = chars[..mid].iter().collect();
        if estimate_tokens(&prefix) <= body_budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    // 预算连一个字符都装不下：只留标注，保证「有内容被截」这一事实可见
    if lo == 0 {
        return SUFFIX.to_string();
    }
    let mut out: String = chars[..lo].iter().collect();
    out.push_str(SUFFIX);
    out
}

/// 组装链内输入：保链头 1 条 + 最近 N 条，双阈取小，中间显式标注省略
///
/// 为什么保链头：链头决定「这整条链在谈什么事」，丢掉它，模型只能看到
/// 尾部几句确认语，必然误判 close。为什么保尾部：新消息是否带来信息，
/// 主要由最近几跳的语义差决定。
pub(crate) fn render_chain(messages: &[ChainMessage], budget: usize) -> ChainInput {
    if messages.is_empty() || budget == 0 {
        return ChainInput {
            rendered: String::new(),
            included: 0,
            omitted: 0,
        };
    }
    let total = messages.len();
    // 条数硬上限（双阈取小的「条数」一侧）：超限时保链头 + 最近 N-1 条，
    // 更早的中间段既不渲染也不占 token 预算。
    let dropped = total.saturating_sub(MAX_CHAIN_MESSAGES);
    let tail_start = if dropped == 0 {
        1
    } else {
        total - (MAX_CHAIN_MESSAGES - 1)
    };
    // 省略标注本身也占预算，先预留最坏情况（全部中间条被省略）
    let reserve = estimate_tokens(&omission_line(total, total));
    // 链头单独超预算时截断：保住「链从哪来」这一端，并给省略标注留出空间，
    // 使「链头 + 省略标注」这条最小渲染恒不越界。
    let head = truncate_to_tokens(&render_one(0, &messages[0]), budget.saturating_sub(reserve));
    let mut used = estimate_tokens(&head);

    let mut tail: Vec<String> = Vec::new();
    let mut idx = total;
    while idx > tail_start {
        let candidate = render_one(idx - 1, &messages[idx - 1]);
        let cost = estimate_tokens(&candidate);
        if used + cost + reserve > budget {
            break;
        }
        used += cost;
        tail.push(candidate);
        idx -= 1;
    }
    tail.reverse();

    let included = 1 + tail.len();
    let omitted = total - included;
    let mut rendered = head;
    if omitted > 0 {
        rendered.push('\n');
        rendered.push_str(&omission_line(omitted, total));
    }
    for line in tail {
        rendered.push('\n');
        rendered.push_str(&line);
    }
    ChainInput {
        rendered,
        included,
        omitted,
    }
}

/// 本场景的策略形态（无状态，可反复构造）
fn policy() -> PickOnePolicy {
    PickOnePolicy::new(
        SCENE,
        "对话链深收敛判定",
        "协作方刚发来的这条消息还有没有新的实质信息、本轮是否还需要自动回发\
（防两个 Agent 沿 reply_to_id 无限互发）",
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
) -> ThinkTraceCerebellum {
    ThinkTraceCerebellum {
        scene: SCENE.to_string(),
        decision: decision.to_string(),
        confidence,
        degraded: degraded.map(|s| s.to_string()),
        applied,
    }
}

/// 本场景的裁决：`close=true` ⇒ 调用方走 `CloseChainAsNotify`
///
/// 只在「判了且要收口」时返回 `Some`；`continue` / 降级 / 未触发一律 `None`
/// （调用方继续走原逻辑）。轨迹里保留细因，调用方无需区分。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChainReplyVerdict {
    pub close: bool,
    pub confidence: f32,
}

/// 生产入口：判一次「还要不要继续回发」（`None` = 走原逻辑）
///
/// `brain` 刻意收 `Option<&Brain>`：调用方直接传 `agent.brain.as_ref()`，
/// 缺失即降级（本门绝不阻断主流程，也不 panic）。
pub(crate) async fn judge_chain_reply(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: Option<&Brain>,
    messages: &[ChainMessage],
    facts: &ChainPressureFacts,
    trace_id: &str,
) -> Option<ChainReplyVerdict> {
    // ① 未达触发阈值：本门不适用 —— 零调用、零配额，但要留痕
    //    （不记这一条，三维观测里就看不出「触发率」，也就无法解释
    //    「为什么这条链一次小脑都没问过」）。
    if !facts.should_ask() {
        record_cerebellum(trace_id, trace(DECISION_NOT_TRIGGERED, None, None, false));
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_NOT_TRIGGERED,
            scene = SCENE,
            chain_depth = facts.chain_depth,
            window_depth = facts.window_depth,
            fatigue = facts.fatigue,
            soft_fatigue = facts.soft_fatigue,
            hard_window_depth = facts.hard_window_depth,
            "chain reply gate not triggered (below both thresholds, zero call)"
        );
        return None;
    }

    // ② 链内一条消息都拉不到（或预算连一条装不下）：不调小脑，落回原阈值
    let input = render_chain(messages, body_token_budget());
    if input.included == 0 {
        record_cerebellum(trace_id, trace(DECISION_EMPTY_INPUT, None, None, false));
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_EMPTY_INPUT,
            scene = SCENE,
            chain_depth = facts.chain_depth,
            "chain reply gate has no chain content (zero call, fall back to static threshold)"
        );
        return None;
    }

    // ③ 未装配小脑 = 降级，不是错误（本门绝不阻断主流程，也不 panic）
    //
    // 刻意写成 `let ... else` 而不是 `brain?`：`?` 在 `Option` 上下文里只是提前
    // `return None`，与 let-else 逐字节等价，不构成「把 None 升级成 Err / 中断主流程」；
    // 此处保留 let-else 只为让每一个降级点都在调用点显式可见。
    #[allow(clippy::question_mark)]
    let Some(brain) = brain else {
        return None;
    };

    // ④ 本 run 的小脑调用配额（上限 = resolved 大脑轮次）：超限静默走原逻辑
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
            trace(DECISION_FALLBACK, None, Some(DEGRADED_BUDGET), false),
        );
        return None;
    }

    let policy = policy();
    let metrics = Metrics::new()
        .with("agent_id", &brain.agent_id)
        .with("agent_name", &brain.agent_name)
        .with("chain_depth", facts.chain_depth as u64)
        .with("window_depth", facts.window_depth as u64)
        .with("fatigue", facts.fatigue)
        .with("soft_fatigue", facts.soft_fatigue)
        .with("hard_fatigue", facts.hard_fatigue)
        .with("hard_window_depth", facts.hard_window_depth)
        .with("chain_messages", &input.rendered);

    // 阶段一：策略声明要问什么（纯函数；选项固定两条，一次 PickOne）
    let requests = policy.prepare(&metrics);
    if requests.is_empty() {
        return None;
    }

    // 阶段二：唯一 async 点（门禁 / 超时 / 协议转换 / 降级全在 DAL）
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_FALLBACK,
            degraded_reason = reason.as_str(),
            scene = SCENE,
            latency_ms = latency_ms,
            included = input.included,
            omitted = input.omitted,
            "chain reply gate degraded to static threshold"
        );
        record_cerebellum(
            trace_id,
            trace(DECISION_FALLBACK, None, Some(reason.as_str()), false),
        );
        return None;
    }

    // 阶段三：答案注入事实桶，形态负责「选一个 + 置信达标 + 不采信幻觉选项」
    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let Some(pick) = policy.chosen(&metrics) else {
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_FALLBACK,
            degraded_reason = DEGRADED_NO_ANSWER,
            scene = SCENE,
            latency_ms = latency_ms,
            "chain reply gate produced no usable answer"
        );
        record_cerebellum(
            trace_id,
            trace(DECISION_FALLBACK, None, Some(DEGRADED_NO_ANSWER), false),
        );
        return None;
    };

    if pick.option == OPTION_CLOSE {
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_CLOSE,
            confidence = pick.confidence,
            scene = SCENE,
            latency_ms = latency_ms,
            chain_depth = facts.chain_depth,
            window_depth = facts.window_depth,
            fatigue = facts.fatigue,
            included = input.included,
            omitted = input.omitted,
            input_tokens = outcome.usage.input_tokens,
            output_tokens = outcome.usage.output_tokens,
            "chain closed by cerebellum (no new information / no need to reply)"
        );
        record_cerebellum(
            trace_id,
            trace(DECISION_CLOSE, Some(pick.confidence), None, true),
        );
        return Some(ChainReplyVerdict {
            close: true,
            confidence: pick.confidence,
        });
    }

    // 放行：下游行为与没有本门时一致，故 `applied=false`；留痕只为算收口率
    // （分母 = 本 scene 轨迹条数，分子 = decision=close）。
    log_info!(
        ctx,
        "chain_reply",
        decision = DECISION_CONTINUE,
        confidence = pick.confidence,
        scene = SCENE,
        latency_ms = latency_ms,
        included = input.included,
        omitted = input.omitted,
        "chain reply gate lets original path run"
    );
    record_cerebellum(
        trace_id,
        trace(DECISION_CONTINUE, Some(pick.confidence), None, false),
    );
    None
}

/// 单例 DAL 的生产封装
pub(crate) async fn judge_chain_reply_with_default_dao(
    ctx: &RequestContext,
    brain: Option<&Brain>,
    messages: &[ChainMessage],
    facts: &ChainPressureFacts,
    trace_id: &str,
) -> Option<ChainReplyVerdict> {
    judge_chain_reply(ctx, dal(), brain, messages, facts, trace_id).await
}

#[cfg(test)]
#[path = "chain_reply_tests.rs"]
mod tests;
