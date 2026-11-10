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
//! - **每次都问**（Q2 改判，AMan 2026-11-10）：只要走到本门（静态规则未命中的
//!   跨 Agent 对等候选）就问一次，不再看 `fatigue` / `stage`。原「`fatigue >=
//!   soft_fatigue` 或 `window_depth >= hard_window_depth - 1`」的门槛已取消——
//!   理由是「有可能从第二句开始就是废话」，挂门槛会漏掉短链。
//! - **`hard_window_depth` 是软墙**（AMan 2026-11-10 定谳，覆盖此前「绝对护栏」
//!   的口径）：小脑判 `continue` 时**即使 `window_depth >= hard_window_depth`**
//!   也放行——判据是内容，跳数只是辅助。静态硬墙不退场，只降为兜底：小脑
//!   `None` / degraded / 超时 / 空输入 / 未配时，调用方照旧落回静态判定
//!   （逐字节不变），这是「小脑挂了也不会无限聊」的保险。
//! - **绝对天花板 [`ABSOLUTE_CEILING`]**：硬墙降软墙之后，小脑持续误判
//!   `continue` 就是物理上的无限互发。故保留一个「无论小脑说什么都强制收口」
//!   的物理上限，命中即收口并记 `degraded = ceiling_forced` 留痕。
//!
//! # 输入侧（Q3 改判：容量上限与服务时延上限**取小**，条数 32 退为防环硬顶）
//!
//! 链内消息由调用方沿 `reply_to_id` 回溯时**顺手收集**（那一趟本来就要拿每条
//! 消息做 `ChainHop` 分类，正文已在手里，零额外读库）。
//!
//! 预算分配是**有界且显式**的，两段取小：
//!
//! ```text
//! 输入预算 = min( 小脑 provider 的推荐上下文长度（容量上限，缺省 max × 60%）, 时延反推上限 )
//! 正文预算 = 输入预算 − 判据骨架预留
//! ```
//!
//! ⚠️ 容量上限是「模型能吃多少」，不是「本次该喂多少」：标准 provider（128K/200K）
//! 的容量恒远大于本场景预算（5 秒）能处理完的量，直接按容量拉满必然超时、功能恒定降级。
//! 因此实际生效值由时延侧（[`LATENCY_INPUT_CAP`]）绑定，容量侧只在**小容量**
//! provider（容量 < 时延上限）时生效。条数 [`MAX_CHAIN_MESSAGES`] = 32 保留为
//! **防环硬顶**（与消费者侧回溯硬上限同口径），不再承担主预算职责。
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

use super::cerebellum_context as context;
use super::compaction::{estimate_tokens, truncate_to_tokens};
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::{AgentRuntimeStateManager, CerebellumGuardVerdict};
use crate::pkg::policy::{Metrics, PickOnePolicy, Policy};
use crate::pkg::think_trace::{ThinkTraceCerebellum, record_cerebellum};
use crate::service::dal::brain::{BrainDal, dal};
use std::sync::Arc;

/// 场景标识（日志 / 计量口径，也是 `DecisionRequest.scene`）
///
/// Q4 定名（**按推荐**）：`chain_reply` —— ①不与既有 scene 撞名（`entry_gate` /
/// `entry_pool` / `thinking_strength.*` / `memory_relevance` / `skill_relevance` /
/// `intent_gate` / `wake_gate` / `summary_gate`）；②描述决策动作本身（这条链
/// 还要不要**回**），而不是描述被判定的事实。刻意不用 `chain_depth`：同一份
/// state 里已有 `chain_depth` / `window_depth` / `hard_window_depth` 三个度量，
/// 再叫 `chain_depth` 会让 `scene × decision × degraded` 与度量字段互相污染。
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
/// 选项 id：静默收口（对端**一条都不发**，只通知归属用户）
///
/// 与 [`OPTION_CLOSE`] 是**两个独立选项**，绝不合并：`close` 仍把本轮 Final 作为
/// 知会送达对端（对方因此知道「到此为止」），`silent` 连这条知会都不发（本条与
/// 本轮回复都没有对方需要知道的内容）。AMan 2026-11-11 定谳：「noreply 是让对方
/// 不回复，不是不回复对方；直接不回复是一个独立的选项」——压成同一个值就算没做。
pub(crate) const OPTION_SILENT: &str = "silent";

/// 轨迹决策值：链内一条消息都拉不到（零调用零配额）
pub(crate) const DECISION_EMPTY_INPUT: &str = "empty_input";
/// 轨迹决策值：模型判继续回发（下游与无本门时一致，`applied=false`）
pub(crate) const DECISION_CONTINUE: &str = "continue";
/// 轨迹决策值：模型判收口
///
/// `applied` 不恒 true：`close` 只在硬墙**之下**才改变下游（静态本是 `Peer`）；
/// 硬墙之上静态判定本就 `Escalate → CloseChainAsNotify`，此时 `close` 是 no-op。
pub(crate) const DECISION_CLOSE: &str = "close";
/// 轨迹决策值：模型判静默收口（对端零发送，只通知归属用户）
///
/// `applied` **恒 `true`**：本门只在「静态规则未命中的跨 Agent 对等候选」上表态 ——
/// 硬墙之下静态本是 `Peer`（被改成对端零发送），硬墙之上静态本是
/// `CloseChainAsNotify`（会发知会，也被改成零发送）。两种情形下游都被改变，
/// 这与 `close` 在硬墙之上是 no-op 的形状不同，故单独定值、不并入。
pub(crate) const DECISION_SILENT: &str = "silent";
/// 轨迹决策值：降级（`degraded` 给出原因）
pub(crate) const DECISION_FALLBACK: &str = "fallback";

/// 降级原因：本 run 小脑调用配额耗尽
const DEGRADED_BUDGET: &str = "budget_exhausted";
/// 降级原因：判了但没有可用结论（低置信 / 答案缺失 / 幻觉选项）
const DEGRADED_NO_ANSWER: &str = "no_answer";
/// 降级原因：端点连续失败已熔断，本次跳过不再尝试（按周期放真探针）
///
/// **独立值域**，绝不并入 `timeout`：熔断是「按设计跳过」，超时是「试了又超」。
/// 并进去之后，「端点挂了 3 次后被跳过」与「第 4 次又超时」在轨迹上一模一样，
/// 排障时只能靠猜。与 [`DEGRADED_BUDGET`] 同理分开：那是成本闸（能问几次），
/// 这是可用性闸（问了也白问）。
const DEGRADED_CIRCUIT_OPEN: &str = "circuit_open";
/// 降级原因：provider 容量连判据骨架都装不下，不构造退化请求
const DEGRADED_CAPACITY: &str = "capacity_too_small";
/// 降级原因：链深触到绝对天花板，**不问小脑**直接强制收口
///
/// 独立值域（不并入 `circuit_open` / `timeout`）：排障时要能一眼分出
/// 「端点坏了被跳过」与「链太长被物理收口」——前者要查 provider，
/// 后者要查小脑判据是否在持续误判 `continue`。
const DEGRADED_CEILING_FORCED: &str = "ceiling_forced";

/// 时延反推的输入 token 上限（= 本次预算的**上界**，不是容量）
///
/// 预算基数 = [`SCENE_TIMEOUT_MS`] = **5000ms**（AMan 2026-11-10 定谳：超时只覆盖
/// 「请求小脑思考」本身，觉得不够就 5 秒，5 秒没反应直接走正常流程；本值是**场景声明**，
/// 与全局缺省 3000ms 相互独立）。
///
/// 拆解（沿用 P1 时延模型）：`5.0s − 网络 0.175s − 生成 0.3s ≈ 4.525s` 可给
/// prefill；按 2000 tok/s 上界 ⇒ 理论 ≈ 9000 tokens。**不取满**：模型侧排队、
/// 生成长度波动、prefill 吞吐在不同 provider 上差 2~3 倍都常见，故取 **4000**：
/// `4000 / 2000 = 2.0s + 0.175s + 0.3s ≈ 2.48s`，对 5s 预算留 ≈ 50% 余量。
///
/// 与旧值的关系：旧值 800 是按当时的 1000ms 反推的（留 20% 余量）；本场景基数
/// 从 1000ms 放到 5000ms 是 5 倍，本常量 800 → 4000 同样是 5 倍——**没有缩回**。
/// 注：全局缺省现为 **3000ms**（AMan 2026-11-10 续令改 3 秒；更早的「1 秒」与
/// 「统一 5 秒」均已被覆盖作废），本场景不吃缺省、只吃 [`SCENE_TIMEOUT_MS`] = 5000ms，
/// 故 4000 仍按 5 秒反推、不受缺省变化影响。
///
/// 容量侧（provider 推荐上下文长度）通常远大于本值，故**实际生效预算由本常量
/// 绑定**；只有小容量 provider 才会被容量侧压下来（[`input_token_budget`] 取小）。
pub(crate) const LATENCY_INPUT_CAP: usize = 4000;

/// 判据骨架的固定开销预留（instructions + options + 压力事实 + 协议包装）
///
/// 取值**由实测定标**（`estimate_tokens` 对真实判据文本计数）：
/// 压缩后的 `INSTRUCTIONS` ≈ 232 + `OPTIONS` ≈ 50 + state ≈ 53 + wrapper ≈ 37
/// = **≈ 372 tokens**；2026-11-10 按 AMan 要求把 `window_depth` / `fatigue`
/// 的「仅作辅助参考」写进 `INSTRUCTIONS`（+两行 ≈ +40 tokens）⇒ ≈ 412，
/// 预留 500 留出 ~88 tokens 缓冲（agent_name 变长、数值进位、协议字段增减）。
///
/// 定标过程本身就是一次纠错：初版 `INSTRUCTIONS` 389 tokens，骨架实测
/// 574 > 预留 400 ⇒ 最坏总输入 974，余量只剩 12~62ms，等于「设计上就
/// 注定超时」。所以判据必须先压，预留才敢按 450 定。
pub(crate) const FRAME_OVERHEAD_RESERVE: usize = 500;

/// 本场景的请求超时预算（毫秒）：**场景级覆盖**，不改全局缺省
///
/// AMan 定谳（2026-11-10）：5 秒只覆盖「请求小脑思考」本身——链内消息准备
/// （`collect_chain_pressure` 那一趟批量查询）发生在 `dal.decide` **之外**，
/// 天然不计入。
///
/// 为什么仍要做成场景级（而不是跟着全局缺省走）：`DEFAULT_CEREBELLUM_TIMEOUT_MS`
/// 是整批共享的一份预算且作用于全部已接 scene，AMan 续令（`01a1c5c9`）已把它抬到
/// 3000ms；本场景需要的是 5000ms（`LATENCY_INPUT_CAP = 4000` token 按 5 秒反推），
/// 比缺省多出的 2 秒只应花在链内多读上下文上，不能让 `entry_gate`（每条入站消息
/// 都过）在端点挂掉时也跟着白等。
pub(crate) const SCENE_TIMEOUT_MS: u64 = 5_000;

/// 绝对天花板：`window_depth` 达到它即**无论小脑说什么都强制收口**
///
/// 硬墙降软墙之后，小脑持续误判 `continue` 就是物理上的无限互发，故保留一个
/// 「按物理极限兜」的上限。取 32 与 [`MAX_CHAIN_MESSAGES`] 对齐——那是消费者侧
/// 沿 `reply_to_id` 回溯的硬上限，超过 32 条的历史本来就已经读不到了，再往上设
/// 天花板没有意义，且会引入第五个数字。
///
/// 与 `hard_window_depth`（缺省 8）的分工：8 是「按轮次猜」的静态判据，会误伤
/// 正常长对话，故降为兜底；32 只在异常（小脑持续误判）时生效，不影响正常对话。
pub(crate) const ABSOLUTE_CEILING: usize = 32;

/// 链内消息条数上限（与 `consumer::message::MAX_CHAIN_TRAVERSAL` 同口径）
///
/// 与 [`body_token_budget`] 是**取小**关系：先按条数裁窗口（保链头 + 最近
/// `MAX_CHAIN_MESSAGES - 1` 条），再按 token 预算裁尾窗口。
pub(crate) const MAX_CHAIN_MESSAGES: usize = 32;

/// 本次判据的输入 token 预算 = **min(容量上限, 时延上限)**
///
/// `capacity_tokens` 是小脑 provider 的推荐上下文长度（容量上限，来自
/// [`ModelProviderConfig::recommended_context_length_tokens`] 这条全局唯一链）。
/// 容量缺失（未配置）时退化到时延上限——保守，绝不按「无限容量 × 5 秒」硬拉。
pub(crate) fn input_token_budget(capacity_tokens: Option<u64>) -> usize {
    capacity_tokens
        .map(|capacity| (capacity as usize).min(LATENCY_INPUT_CAP))
        .unwrap_or(LATENCY_INPUT_CAP)
}

/// 正文预算的**最低可用线**：低于它就不问，而不是发一个只剩骨架的请求
///
/// `body_token_budget` 是裸减法（`input − 450`），小容量 provider（配置写几百
/// token 的模型）会算出 0 或个位数。此时 `render_chain` 仍会返回 `included = 1`，
/// 但正文只剩 `…（正文已截断）` 这一行 —— 请求发得出去、账单照付，模型拿到的
/// 却是一句「正文已截断」，判 `close` 与否纯属噪声。所以这里把「预算不够」
/// 显式变成「不问」（与未装配小脑同一语义：不渲染、不占配额、不发请求）。
///
/// 取 100 的定标依据：省略标注行本身约 40 tokens（`[中间 N 条已省略（共 M 条
/// 链内消息；仅保留链头与最近若干条）]`），加一条短消息正文约 20~40 tokens，
/// 100 才容得下「链头 + 标注 + 至少一条真实正文」这一最小可用组合。
pub(crate) const MIN_BODY_TOKEN_BUDGET: usize = 100;

/// 链内正文可用的 token 预算 = 输入预算 − 骨架预留（不足时归零，不下溢）
pub(crate) fn body_token_budget(input_budget: usize) -> usize {
    input_budget.saturating_sub(FRAME_OVERHEAD_RESERVE)
}

/// 判据（prompt 存调用方，服务端不持有）
///
/// 只写黑名单侧（默认 continue）：误收口的代价高于多回一轮。
/// 末段「省略标注 ⇒ 证据不足选 continue」是本场景唯一的假阴性防线。
const INSTRUCTIONS: &str = "判断协作方刚发来的消息有没有新的实质信息、本轮还需不需要自动回发；\
只看链内消息本身。\n\
选 silent：本条与本轮回复里都没有对方需要知道的内容（纯确认 / 纯致谢 / 纯重复），\n\
连「到此为止」都不必告诉对方。\n\
选 close：只是确认 / 致谢 / 收讫 / 复述（「收到」「明白」「已完成」），但对方需要知道「到此为止」，\
或结论已在上文交付、本条只是重复总结，或同一层意思反复换措辞。\n\
选 continue：带来了上文没有的新事实 / 数据 / 结论 / 问题 / 待办，\
或提出需要答复的问题、需要确认的决策点、需要执行的下一步。\n\
判不准就选 continue —— 误收口代价高于多回一轮。\n\
state 里的 window_depth（当前链深度）与 fatigue（疲劳值）**仅作辅助参考**：\n\
主判据是这条消息有没有新的实质信息量，不得仅因为这两个数字偏大就选 close。\n\
链内区块若出现「中间 N 条已省略」，不得把「看不到」当成「没有新信息」，\
证据不足一律选 continue。";

/// 三个选项（id → 判据描述）
///
/// `continue` / `close` 的 id 与语义**零改动**（旧轨迹照旧可解读），只新增
/// `silent`。`close` 与 `silent` 的判据刻意写成对照对：「对方是否需要知道
/// 『到此为止』」——这是模型唯一可判的差别，也是本改动最大的风险面
/// （两者区分度靠这一句，而不是靠 confidence）。
const OPTIONS: [(&str, &str); 3] = [
    (
        OPTION_CONTINUE,
        "继续回发：带来上文没有的新事实 / 结论 / 问题 / 待办，或证据不足（含链内被截断）",
    ),
    (
        OPTION_CLOSE,
        "不再回发：只是确认 / 致谢 / 收讫 / 复述，或同一层意思反复换措辞",
    ),
    (
        OPTION_SILENT,
        "连「到此为止」都不必告诉对方：本条与本轮回复都没有对方需要知道的内容（纯确认 / 纯致谢 / 纯重复）",
    ),
];

/// 透传给模型的上下文：**单键分块正文**（走组织器 `cerebellum_context` 的统一出口）
///
/// 改造前是 9 键平铺（`agent_id` / `agent_name` / 6 个压力数字 / `chain_messages`）：
/// 同一种「给模型读的上下文」在 7 个接入点里出现了两套写法。收敛后本场景与另外
/// 六个场景同形，唯一装配点是 [`state_bundle`]。
///
/// ⚠️ 宣称边界：本笔之后「全仓**代码路径**单出口」成立（残留 3 条 = 2 条 doc /
/// 注释 + 1 条无关测试）；但这不等于「所有接入点都接了容量自截」——本场景刻意
/// 不用组织器的 `render_within`，理由见 [`state_bundle`]。
const STATE_KEYS: &[&str] = &[context::STATE_KEY];

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

// ⚠️ Q2 改判（AMan 2026-11-10）：原 `should_ask` 门槛（`fatigue >= soft` 或
// `window_depth >= hard-1`）**已取消**。走到本门即问一次——「有可能从第二句
// 开始就是废话」，挂门槛会漏掉短链。压力事实仍全部注入 state，供判据参考。

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

/// 链内正文的截断标注（实现见 `compaction::truncate_to_tokens`，
/// 小脑 state 裁剪共用同一实现、只差这段文案）
const BODY_TRUNCATION_SUFFIX: &str = "…（正文已截断）";

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
    let head = truncate_to_tokens(
        &render_one(0, &messages[0]),
        budget.saturating_sub(reserve),
        BODY_TRUNCATION_SUFFIX,
    );
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

/// 协作压力块正文：六个数字逐行 `key: value`
///
/// 六个全部保留 —— `window_depth` / `fatigue` 在判据里写明「仅作辅助参考」，但
/// `soft_*` / `hard_*` 是模型理解「这一跳处在第几层」的唯一依据；少给一个，
/// 它就会把当前跳数当绝对值读。
fn pressure_body(facts: &ChainPressureFacts) -> String {
    format!(
        "chain_depth: {}\nwindow_depth: {}\nfatigue: {}\nsoft_fatigue: {}\n\
hard_fatigue: {}\nhard_window_depth: {}",
        facts.chain_depth,
        facts.window_depth,
        facts.fatigue,
        facts.soft_fatigue,
        facts.hard_fatigue,
        facts.hard_window_depth
    )
}

/// 本场景的上下文装配（**唯一装配点**）：块序固定
/// `会话身份` → `当前场景` → `链内消息` → `协作压力`
///
/// 两处刻意的「不」：
/// - **不用 `with_user_message`**：本场景的输入是 N 条链内历史正文，不是「当前用户
///   消息」；那个标题会把多轮历史误导成一条待判正文。
/// - **不用 `with_recent_dialogue`（`Brain.memories`）**：链内正文已是主信息源，再叠
///   记忆摘要属**新增判断输入**（语义变更）而非形态迁移 ⇒ 单独评估，不夹带。
///
/// 也**不做组织器侧容量自截**（`render_within`）：链内正文已由 [`render_chain`] 按
/// `body_token_budget` 自截到 `容量 − FRAME_OVERHEAD_RESERVE` 以内；再叠一层
/// `容量 − CONTEXT_FRAME_RESERVE_TOKENS(1024)` 的**尾块整砍**，会在中间容量带把
/// `## 链内消息` 这块主要证据整块砍掉 —— 比不截断更坏（实测见 `chain_reply_tests`）。
/// 容量安全的判据是「`body_token_budget < MIN_BODY_TOKEN_BUDGET` ⇒ 不问」，不是
/// 「截到装得下」。
fn state_bundle(
    brain: &Brain,
    chain_rendered: &str,
    facts: &ChainPressureFacts,
) -> context::ContextBundle {
    let mut bundle = context::ContextBundle::new()
        .with_identity(&brain.agent_id, &brain.agent_name)
        .with_scene(SCENE);
    // 空正文（一条链内消息都没带上）时 `push` 整块跳过，不留 `## 链内消息` 裸标题
    bundle.push("链内消息", chain_rendered.to_string());
    bundle.push("协作压力", pressure_body(facts));
    bundle
}

/// 本场景的 state 装配（单出口：`{"context": "<分块正文>"}`）
fn state_metrics(brain: &Brain, chain_rendered: &str, facts: &ChainPressureFacts) -> Metrics {
    Metrics::new().with(
        context::STATE_KEY,
        state_bundle(brain, chain_rendered, facts).render(),
    )
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
    // 场景级超时覆盖：只在 chain_reply 生效，全局缺省不动
    .with_timeout_ms(SCENE_TIMEOUT_MS)
}

/// 一条小脑留痕（三态 + 降级共用一个构造器，避免字段漏填）
///
/// `latency_ms` 对**没发生调用**的分支传 `None`（空输入 / 配额 / 熔断 / 容量不足），
/// 对发生过调用的分支一律传 `Some(outcome.latency_ms)` —— 含 `applied=false`
/// 的放行与 `degraded` 的降级：它们同样付了时延，漏记会让 p95 系统性低估。
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

/// 本场景的裁决（三个「有结论」分支）
///
/// 返回值 `None` = 没结论 ⇒ 调用方走静态判定（含硬墙 `Escalate`），逐字节不变。
/// `Some(Close)` ⇒ `CloseChainAsNotify`。
/// `Some(Silent)` ⇒ `SilentClose`（对端零发送，只通知归属用户）。
/// `Some(Continue)` ⇒ **直接 `Peer`**，跳过静态判定 —— 这是「硬墙降软墙」的落点：
/// 静态硬墙只在「小脑没结论」时生效。
///
/// **`applied` 的统一口径 = 本门是否改变了下游**（四条定值，`depth = window_depth`）：
/// - `close` + `depth <  hard` ⇒ `true`（静态本是 `Peer`，被改成收口）
/// - `close` + `depth >= hard` ⇒ `false`（静态本也 `Escalate`，下游不变）
/// - `continue` + `depth >= hard` ⇒ `true`（越墙，软墙真起效）
/// - `continue` + `depth <  hard` ⇒ `false`（墙下本就走，没改）
/// - `silent`（任意 `depth`）⇒ `true`（墙下静态本 `Peer`、墙上静态本
///   `CloseChainAsNotify` 会发知会，两种情形都被「对端零发送」改变）
///
/// 推论（读三维立方最容易看错的一处）：`depth >= hard` 区间里只有 `continue` 能
/// 改变下游，`close` 全是 no-op。天花板 `ceiling_forced` 也落在该区间、且根本没问
/// 小脑 ⇒ 同样 `applied = false`（`degraded` + `false` 合起来才是「未生效但留痕」）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ChainReplyVerdict {
    /// 收口；`confidence` 为 `None` = 非小脑判断（绝对天花板强制）
    Close {
        /// 小脑判断置信度（天花板强制时无判断，为 `None`）
        confidence: Option<f32>,
    },
    /// 静默收口：对端**零发送**（连知会都不发），只通知归属用户（保消息必达）
    ///
    /// 与 [`Self::Close`] 独立、不合并：`close` 把 Final 作为知会送达对端，
    /// `silent` 连知会都不发。两者各自独立计数（`decision` 三值分列）。
    Silent {
        /// 小脑判断置信度
        confidence: f32,
    },
    /// 继续回发：**越过静态硬墙**（软墙）
    Continue {
        /// 小脑判断置信度
        confidence: f32,
    },
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
    // ① 未装配 Brain = 降级，不是错误（本门绝不阻断主流程，也不 panic）
    //
    // 刻意写成 `let ... else` 而不是 `brain?`：`?` 在 `Option` 上下文里只是提前
    // `return None`，与 let-else 逐字节等价，不构成「把 None 升级成 Err / 中断主流程」；
    // 此处保留 let-else 只为让每一个降级点都在调用点显式可见。
    #[allow(clippy::question_mark)]
    let Some(brain) = brain else {
        return None;
    };

    // ② 未启用小脑模型 = **零开销**（不渲染、不占配额、不发请求）
    //
    // 「每次都问」之后这条变得关键：没配小脑的 Agent 不能每条跨 Agent 回复都吃
    // 一次「渲染 + 请求 + 等待」。确认有模型才继续往下走。
    // 与上面 `brain` 同一条口径：显式 let-else 胜在「每个降级点都在调用点可见」，
    // `?` 会把它压成一行而看不出这是零开销的守卫。
    #[allow(clippy::question_mark)]
    if brain.cerebellum.is_none() {
        return None;
    }

    // ②.5 绝对天花板：硬墙降软墙之后，小脑的 `continue` 可以越墙 ⇒ 必须有
    // 一个「无论小脑说什么都收口」的物理上限，否则小脑持续误判就是真无限互发。
    //
    // 放在 decide **之前**而不是之后：到顶时结论已定（必收口），再花一次 5s
    // 预算去问一个必然被覆盖的问题，纯粹是白等 + 白计一次配额。
    if facts.window_depth >= ABSOLUTE_CEILING {
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_FALLBACK,
            degraded_reason = DEGRADED_CEILING_FORCED,
            scene = SCENE,
            window_depth = facts.window_depth,
            fatigue = facts.fatigue,
            absolute_ceiling = ABSOLUTE_CEILING,
            hard_window_depth = facts.hard_window_depth,
            "chain depth hits the absolute ceiling: close regardless of cerebellum verdict"
        );
        record_cerebellum(
            trace_id,
            // `applied = false`：天花板在 `depth >= 32 >= hard(8)`，该区间静态判定
            // 本就 `Escalate → CloseChainAsNotify`，下游**没变**；且本态根本没问小脑
            // （`degraded = ceiling_forced`），记 true 等于把「没判」记成「生效」，
            // 会污染生效率。`degraded` + `applied = false` 合起来才是「未生效但留痕」。
            trace(
                DECISION_FALLBACK,
                None,
                Some(DEGRADED_CEILING_FORCED),
                false,
                None,
            ),
        );
        return Some(ChainReplyVerdict::Close { confidence: None });
    }

    // ③ 熔断：端点连续失败后跳过（按周期放真探针自愈）
    //
    // 位置极关键 —— 必须在渲染 / 配额 / 请求**之前**（②.5）：熔断的全部价值
    // 就是「别再付那一个超时」，放在 decide 旁边等于白熔。也必须在
    // `cerebellum.is_none()` 之后：没有模型时熔断不欠任何人一个解释。
    //
    // 载体是 `AgentRuntimeStateManager`（键 = 本 Brain 的宿主 agent_id），不是
    // Brain 上的字段：`handle_agent_message` **每条入站消息都重建 Brain**，挂在
    // Brain 上的连续失败计数取值恒为 0 或 1，阈值 3 在生产接线形态下永远不可达
    // （单测里循环调同一个 Brain 能过 ⇒ 典型假绿）。运行时状态是纯内存单例、
    // 跨消息累计、服务重启归零，满足「不入 DB / 不设时间窗」。
    let runtime_state = AgentRuntimeStateManager::global();
    let guard_verdict = runtime_state.cerebellum_guard_verdict(&brain.agent_id);
    if matches!(guard_verdict, CerebellumGuardVerdict::Skip) {
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_FALLBACK,
            degraded_reason = DEGRADED_CIRCUIT_OPEN,
            scene = SCENE,
            consecutive_failures = runtime_state.cerebellum_consecutive_failures(&brain.agent_id),
            skipped_since_probe = runtime_state.cerebellum_skipped_since_probe(&brain.agent_id),
            probe_every = crate::pkg::agent_runtime_state::CEREBELLUM_PROBE_EVERY,
            "chain reply gate skipped by circuit breaker (endpoint keeps failing)"
        );
        record_cerebellum(
            trace_id,
            trace(
                DECISION_FALLBACK,
                None,
                Some(DEGRADED_CIRCUIT_OPEN),
                false,
                None,
            ),
        );
        return None;
    }
    if matches!(guard_verdict, CerebellumGuardVerdict::Probe) {
        // 探针：真发一次请求。成功即清零回正常，失败则继续开闸并重新计周期。
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_FALLBACK,
            degraded_reason = "circuit_probe",
            scene = SCENE,
            consecutive_failures = runtime_state.cerebellum_consecutive_failures(&brain.agent_id),
            probe_every = crate::pkg::agent_runtime_state::CEREBELLUM_PROBE_EVERY,
            "circuit breaker probe: retrying the endpoint once to check recovery"
        );
    }

    // ④ 输入预算 = min(小脑模型容量上限, 时延反推上限)
    //    容量对标准 provider 恒远大于时延上限 ⇒ 实际生效值由时延侧绑定。
    let capacity_tokens = brain
        .cerebellum
        .as_ref()
        .and_then(|po| po.config().recommended_context_length_tokens());
    let input_budget = input_token_budget(capacity_tokens);
    let body_budget = body_token_budget(input_budget);

    // ⑤ 容量连判据骨架都装不下：不问，也不构造退化请求
    //
    // 裸减法（`input − 450`）在小容量 provider 上会算出 0 或个位数；此时
    // `render_chain` 仍会给出 `included = 1`，但正文只剩「…（正文已截断）」，
    // 请求发得出去、账单照付、结论纯噪声。显式变成「不问」比「问了也白问」诚实。
    if body_budget < MIN_BODY_TOKEN_BUDGET {
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_FALLBACK,
            degraded_reason = DEGRADED_CAPACITY,
            scene = SCENE,
            capacity_tokens = capacity_tokens.unwrap_or(0),
            input_budget = input_budget,
            body_budget = body_budget,
            min_body_budget = MIN_BODY_TOKEN_BUDGET,
            "chain reply gate skipped: provider capacity below judge skeleton"
        );
        record_cerebellum(
            trace_id,
            trace(
                DECISION_FALLBACK,
                None,
                Some(DEGRADED_CAPACITY),
                false,
                None,
            ),
        );
        return None;
    }

    // ⑥ 链内一条消息都拉不到：不调小脑、不占配额
    let input = render_chain(messages, body_budget);
    if input.included == 0 {
        // 空输入没发请求、也没证明端点坏了 ⇒ 按裁定口径记「健康」（清零）：
        // 只有 timeout / call_error 才是熔断证据，其余一律不累加。
        runtime_state.record_cerebellum_healthy(&brain.agent_id);
        record_cerebellum(
            trace_id,
            trace(DECISION_EMPTY_INPUT, None, None, false, None),
        );
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

    // ⑦ 本 run 的小脑调用配额（上限 = resolved 大脑轮次）：超限静默走原逻辑
    //
    // 放在「本地就能判完」的两道门（未启用小脑 / 空输入）之后：配额是有限
    // 保护资源，不该被本地短路的情形白吃一次。
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
    let metrics = state_metrics(brain, &input.rendered, facts);

    // 阶段一：策略声明要问什么（纯函数；选项固定三条，一次 PickOne）
    let requests = policy.prepare(&metrics);
    if requests.is_empty() {
        return None;
    }

    // 阶段二：唯一 async 点（门禁 / 超时 / 协议转换 / 降级全在 DAL）
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;

    // 熔断记账：只认**端点级**失败（超时 / 调用失败）。
    // 「有应答但没结论」（低置信 / 幻觉选项）视为端点健康 → 清零：
    // 熔断判的是端点可用性，不是判断质量；把低置信也算失败，会在模型偶发
    // 保守时误封一个其实活着的端点。
    if matches!(
        outcome.degraded,
        Some(common::api::cerebellum::DegradedReason::Timeout)
            | Some(common::api::cerebellum::DegradedReason::CallError)
    ) {
        runtime_state.record_cerebellum_transport_failure(&brain.agent_id);
    } else {
        // 有应答即清零：低置信 / 幻觉选项 / close / continue 都走这里 ——
        // 熔断判的是端点可用性，不是判断质量。
        runtime_state.record_cerebellum_healthy(&brain.agent_id);
    }

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
            trace(
                DECISION_FALLBACK,
                None,
                Some(reason.as_str()),
                false,
                Some(latency_ms),
            ),
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
            trace(
                DECISION_FALLBACK,
                None,
                Some(DEGRADED_NO_ANSWER),
                false,
                Some(latency_ms),
            ),
        );
        return None;
    };

    // `applied` 的统一口径 = 本门是否**改变了下游**（四条定值见本函数 doc）。
    // 提前算一次供 close / continue 共用：硬墙之上静态判定本就收口，close 在那
    // 里是 no-op，只有 continue 能改变下游。
    let overrode_static_wall = facts.window_depth as u64 >= facts.hard_window_depth;

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
            hard_window_depth = facts.hard_window_depth,
            overrode_static_wall = overrode_static_wall,
            fatigue = facts.fatigue,
            included = input.included,
            omitted = input.omitted,
            input_budget = input_budget,
            input_tokens = outcome.usage.input_tokens,
            output_tokens = outcome.usage.output_tokens,
            "chain closed by cerebellum (no new information / no need to reply)"
        );
        record_cerebellum(
            trace_id,
            trace(
                DECISION_CLOSE,
                Some(pick.confidence),
                None,
                !overrode_static_wall,
                Some(latency_ms),
            ),
        );
        return Some(ChainReplyVerdict::Close {
            confidence: Some(pick.confidence),
        });
    }

    // 静默收口：对端零发送。与 close 的差别只在「有没有一条需要告诉对方的话」，
    // 路由层是两个 enum 变体、轨迹层是两个 decision 值 —— 不许压成同一个值。
    if pick.option == OPTION_SILENT {
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_SILENT,
            confidence = pick.confidence,
            scene = SCENE,
            latency_ms = latency_ms,
            chain_depth = facts.chain_depth,
            window_depth = facts.window_depth,
            hard_window_depth = facts.hard_window_depth,
            fatigue = facts.fatigue,
            included = input.included,
            omitted = input.omitted,
            input_budget = input_budget,
            input_tokens = outcome.usage.input_tokens,
            output_tokens = outcome.usage.output_tokens,
            "chain closed silently by cerebellum (nothing the peer needs to know)"
        );
        record_cerebellum(
            trace_id,
            trace(
                DECISION_SILENT,
                Some(pick.confidence),
                None,
                true,
                Some(latency_ms),
            ),
        );
        return Some(ChainReplyVerdict::Silent {
            confidence: pick.confidence,
        });
    }

    // 放行：小脑判 continue ⇒ 下游**直接 Peer**，不再落回静态判定。
    //
    // 与旧版的差别（AMan 2026-11-10 定谳）：旧版返回 `None` 让调用方走静态判定，
    // 于是 `window_depth >= hard_window_depth` 时会被静态 `Escalate` 收口 ——
    // 那等于「小脑说还能聊，但硬墙说了算」。现在反过来：硬墙降为兜底，只在
    // 小脑没结论时生效。
    //
    // `applied` 的语义：本门是否**改变了**下游。判 continue 且在硬墙之下 ⇒
    // 下游本来也是 Peer（`applied=false`）；判 continue 且越过硬墙 ⇒ 下游本来
    // 会 `CloseChainAsNotify`，被本门改回 Peer（`applied=true`）——这一位就是
    // 「软墙真的起效了」的可观测点。
    log_info!(
        ctx,
        "chain_reply",
        decision = DECISION_CONTINUE,
        confidence = pick.confidence,
        scene = SCENE,
        latency_ms = latency_ms,
        window_depth = facts.window_depth,
        hard_window_depth = facts.hard_window_depth,
        overrode_static_wall = overrode_static_wall,
        included = input.included,
        omitted = input.omitted,
        "chain reply gate lets the chain continue (soft wall)"
    );
    record_cerebellum(
        trace_id,
        trace(
            DECISION_CONTINUE,
            Some(pick.confidence),
            None,
            overrode_static_wall,
            Some(latency_ms),
        ),
    );
    Some(ChainReplyVerdict::Continue {
        confidence: pick.confidence,
    })
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
