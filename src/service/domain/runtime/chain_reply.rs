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
//! **静态阈值完整保留为兜底**：小脑没配 / 失败 / 超时 / 空输入 → 落回原逻辑；
//! **答了但不可用**（没答 / 该选项置信不达标 / 幻觉选项）→ 默认 `close`（仅通知），
//! 因为它是三个选项里代价最小的一个（Final 仍作为知会送达对端）。
//!
//! # 落点与调用顺序（由 `consumer::message` 保证）
//!
//! ```text
//! 静态规则未命中（= 跨 Agent 对等候选）
//!   → collect_chain_pressure 得 pressure（+ 顺手收链内正文，零额外 DB 读）
//!   → 小脑可用：本模块判一次
//!        close    → CloseChainAsNotify（复用现成收口）
//!        silent   → SilentClose（对端零发送）
//!        continue → Peer（越过静态硬墙）
//!        没答 / 置信不达标 / 幻觉 → 默认 close
//!        调用失败 / 超时 / 空输入 / 未配 → 落到下一步
//!   → judge_chain_reply_route(pressure)  ← 原样，逐字节不变
//! ```
//!
//! **判据只用一次 `PickOne`**，候选三条（`continue` / `close` / `silent`），
//! 不逐候选各发一题。
//!
//! # 触发阈值
//!
//! - **每次都问**（Q2 改判，AMan 2026-11-10）：只要走到本门（静态规则未命中的
//!   跨 Agent 对等候选）就问一次，不再看 `fatigue` / `stage`。原「`fatigue >=
//!   soft_fatigue` 或 `window_depth >= hard_window_depth - 1`」的门槛已取消——
//!   理由是「有可能从第二句开始就是废话」，挂门槛会漏掉短链。
//! - **`hard_window_depth` 是软墙**（AMan 2026-11-10 定谳，覆盖此前「绝对护栏」
//!   的口径）：小脑判 `continue` 时**即使 `window_depth >= hard_window_depth`**
//!   也放行——判据是内容，跳数只是辅助。静态硬墙不退场，只降为兜底：小脑
//!   没参与（失败 / 超时 / 空输入 / 未配）时，调用方照旧落回静态判定
//!   （逐字节不变），这是「小脑挂了也不会无限聊」的保险。
//! - **绝对天花板 [`ABSOLUTE_CEILING`]**：硬墙降软墙之后，小脑持续误判
//!   `continue` 就是物理上的无限互发。故保留一个「无论小脑说什么都强制收口」
//!   的物理上限，命中即收口并记 `degraded = ceiling_forced` 留痕。
//!
//! # 置信门槛（按选项分档）
//!
//! 三个选项的代价不同，故不共用一条门槛：
//!
//! | 选项 | 门槛 | 代价 |
//! |---|---|---|
//! | `close` | 0（默认采纳/默认兜底） | 最低：Final 仍作为知会送达，链不再增长 |
//! | `silent` | 0.8 | 中等：对端零发送，误判后不会再被唤醒 |
//! | `continue` | 0.9 | 最高：多跑一轮完整思考，且链深继续增长 |
//!
//! 「答了但选中项不达标」与「没答」都落到 `close`——拿不准就用代价最小的姿态。
//!
//! # 输入侧（容量即预算；条数 32 退为防环硬顶）
//!
//! 链内消息由调用方沿 `reply_to_id` 回溯时**顺手收集**（那一趟本来就要拿每条
//! 消息做 `ChainHop` 分类，正文已在手里，零额外读库）。
//!
//! 预算分配是**有界且显式**的，按 provider 容量比例分账：
//!
//! ```text
//! 输入预算 = 小脑 provider 的推荐上下文长度（容量上限，缺省 max × 60%；未配置 ⇒ 不裁）
//! 正文预算 = 输入预算 − 判据骨架（输入 × FRAME_BP）
//! ```
//!
//! ⚠️ 沿革：本场景曾按「5 秒超时 × 实测 prefill 速率」反推一个 4000 token 的时延上限。
//! 速率项无实测定标（属经验假设），AMan 2026-11-11 终裁**整条轴删除**——容量即预算，
//! 不再用时间预算乘速率去猜 token 数。时间预算只剩 [`SCENE_TIMEOUT_MS`]（请求超时，
//! 不是 token 预算）。条数 [`MAX_CHAIN_MESSAGES`] = 32 保留为**防环硬顶**
//! （与消费者侧回溯硬上限同口径），不再承担主预算职责。
//!
//! 超长降级：**保链头 [`HEAD_KEEP`] 条 + 最近 N 条**，中间被丢弃的部分必须在 prompt
//! 里显式写出「中间 N 条已省略（共 M 条）」。不标注，小脑会把「看不到」当成
//! 「没有新信息」；判据里因此写死「无法确认时选 close（仅通知）」。
//!
//! # 降级不变量（三条，与《小脑快判断的运行口径》一致）
//!
//! 1. 小脑没参与 → `None` 走原逻辑；小脑参与了但**答不可用** → 默认 `close`
//!    （代价最小的业务结论），不再落回静态；
//! 2. 三态严格分：`degraded`（没判出来）/ `applied=false`（判了但未改变下游，
//!    含 `continue` 在硬墙之下）/ 正常（结论改变了下游）；
//! 3. 收口类决策必须自补下游最小输入：`close` 走的是**现成**收口路径
//!    （`CloseChainAsNotify`：Final 仍作为知会送达对方 + 通知归属用户），
//!    因此「消息必达」语义不变、收口路径仍可用。
//!
//! # 观测
//!
//! 每次进入本门都写一条 `record_cerebellum{scene: "chain_reply", decision}`。
//! `decision` 六值 = `not_triggered` / `empty_input` / `continue` / `close` /
//! `close_default` / `fallback`。**记账条目同样占轨迹名额**
//! —— 一次唤醒最多 1 条（一次回发判定一次），单 run 32 条上限内绰绰有余。
//! `close_default` 与 `close` 必须分列：前者是「没判出来」的兜底占比，正是
//! 判断「硬墙能否退场」的依据。

use super::cerebellum_context as context;
use super::compaction::{estimate_tokens, truncate_to_tokens};
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::{AgentRuntimeStateManager, CerebellumGuardVerdict};
use crate::pkg::policy::{DecisionPolicy, DecisionValue, Metrics, PickOne, PickOnePolicy, Policy};
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

/// 默认置信阈值：未单列门槛的选项使用它
///
/// 取 0.9 是本场景历史口径，现在只作为兜底（三个选项都已单列，见
/// [`MIN_CONFIDENCE_CLOSE`] / [`MIN_CONFIDENCE_SILENT`] / [`MIN_CONFIDENCE_CONTINUE`]）。
pub(crate) const MIN_CONFIDENCE: f32 = 0.9;

/// `close` 的置信门槛：**0 = 任何时候都采纳**
///
/// 代价最低的选项：Final 仍作为知会送达对端（信息不丢），只是不再自动回发，
/// 链深不会再增长。因此它既是「模型选它就直接采纳」，也是「别的选项不达标」时的
/// 默认落点（见 [`DECISION_CLOSE_DEFAULT`]）。
pub(crate) const MIN_CONFIDENCE_CLOSE: f32 = 0.0;

/// `silent` 的置信门槛：中等代价
///
/// 对端一条消息都收不到，误判后它既不知道链已结束、也不会再被唤醒；代价高于
/// `close`（少一条知会），低于 `continue`（后者会让链继续变深）。
pub(crate) const MIN_CONFIDENCE_SILENT: f32 = 0.8;

/// `continue` 的置信门槛：最高代价
///
/// 每多回一跳都要再付一次完整思考 + 投递成本，并且链深继续增长；且本门已把
/// 静态硬墙降为软墙，`continue` 会越过它。故必须有高置信才放行。
pub(crate) const MIN_CONFIDENCE_CONTINUE: f32 = 0.9;

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
/// 轨迹决策值：**没拿到可用结论时的默认收口**
///
/// 与 [`DECISION_CLOSE`] 分开计数：那个是模型自己选了 `close`，这个是「答案缺失 /
/// 置信不达标 / 幻觉选项」时按最保守选项兜底。两者的生效率与置信分布完全不同，
/// 混在一起就无法观察「默认收口」到底占多少（也就无法判断硬墙能否退场）。
pub(crate) const DECISION_CLOSE_DEFAULT: &str = "close_default";
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
/// 降级原因：判了但没有可用结论（答案缺失 / 幻觉选项）
const DEGRADED_NO_ANSWER: &str = "no_answer";
/// 降级原因：答了，但选中项的置信度没达到该选项自己的门槛
///
/// 与 [`DEGRADED_NO_ANSWER`] 分列：前者是「没答出来」，后者是「答了但不够确定」，
/// 排障时一个要查协议 / 端点，另一个要查判据与阈值标定。
const DEGRADED_LOW_CONFIDENCE: &str = "low_confidence";
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

// 骨架预留不再有本场景私有常量：改由组织器的比例表派生
// （`context::share::FRAME_BP` = 12.5%，@输入 4000 ⇒ 500，与改造前逐位一致）。
// 比例是唯一口径：大容量下骨架随容量线性增长，不再有绝对封顶。

/// 本场景的请求超时预算（毫秒）：**场景级覆盖**，不改全局缺省
///
/// AMan 定谳（2026-11-10）：5 秒只覆盖「请求小脑思考」本身——链内消息准备
/// （`collect_chain_pressure` 那一趟批量查询）发生在 `dal.decide` **之外**，
/// 天然不计入。
///
/// 为什么仍要做成场景级（而不是跟着全局缺省走）：`DEFAULT_CEREBELLUM_TIMEOUT_MS`
/// 是整批共享的一份预算且作用于全部已接 scene，AMan 续令（`01a1c5c9`）已把它抬到
/// 3000ms；本场景需要的是 5000ms（整条链最多 32 条消息一次读入，读的量比其他场景
/// 大得多），比缺省多出的 2 秒只应花在链内多读上下文上，不能让 `entry_gate`
/// （每条入站消息都过）在端点挂掉时也跟着白等。
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
/// `MAX_CHAIN_MESSAGES - HEAD_KEEP` 条），再按 token 预算裁尾窗口。
pub(crate) const MAX_CHAIN_MESSAGES: usize = 32;

/// 链内被裁时**无条件保留的链头条数**
///
/// 只保 1 条太薄：首条通常只是打招呼 / 交代一句，看不出议题与分工。保 3 条
/// 才能回答「这条链在解决什么、双方各是谁」。代价只是多占一点正文预算。
pub(crate) const HEAD_KEEP: usize = 3;

/// 本场景的「最近对话」条数
///
/// 通用口径 [`context::RECENT_DIALOGUE_LIMIT`] = 3 是给另外六个接入点的保守值；
/// 本场景要判「这条消息有没有新信息」，3 条短期记忆摘要常常不够覆盖链起点前后
/// 的语义，故单列 8 条（同一份 `Brain.memories`，零额外 IO）。
pub(crate) const CHAIN_RECENT_DIALOGUE_LIMIT: usize = 8;

/// 本次判据的输入 token 预算 = **provider 推荐上下文长度**（容量即预算）
///
/// `capacity_tokens` 是小脑 provider 的推荐上下文长度（容量上限，来自
/// [`ModelProviderConfig::recommended_context_length_tokens`] 这条全局唯一链）。
/// 容量未配置 ⇒ 不裁（`usize::MAX`）：既没有容量也没有时延轴可依赖，猜一个数
/// 只会再现「拍脑袋上限」这笔债——「容量未配不猜」是本仓既定口径。
///
/// 沿革：本函数曾是 `min(容量, 4000 时延上限)`。时延轴已按 AMan 2026-11-11 终裁
/// 删除（速率项无实测定标），故只剩容量。
pub(crate) fn input_token_budget(capacity_tokens: Option<u64>) -> usize {
    capacity_tokens
        .map(|capacity| capacity as usize)
        .unwrap_or(usize::MAX)
}

// 正文预算不再有「最低可用线」这个绝对值：AMan 2026-11-11 终裁删掉 100。
// 风险改由**结构性判据**兜住：`body_token_budget == 0` ⇒ 不问（见 `decide` 第⑤步）。

/// 链内正文可用的 token 预算 = 输入预算 − 骨架预留（比例派生，见 [`context::body_budget_tokens`]）
///
/// 骨架预留 = `输入 × 12.5%`（纯比例，无绝对值封顶）：输入大时骨架随容量线性
/// 增长，输入小时按同一比例缩到很小 —— 不再是一刀切的固定 500（那会让小容量
/// provider 的正文预算恒为 0、永远降级）。
pub(crate) fn body_token_budget(input_budget: usize) -> usize {
    context::body_budget_tokens(input_budget)
}

// 待发正文的子预算不再有本场景私有常量：改由组织器的比例表派生
// （`context::share::PENDING_REPLY_BP` = 15%，@输入 4000 ⇒ 600，与改造前逐位一致；
// 再被正文池的 1/4 夹住，小容量自动收敛）。
// 存在理由不变：判 ①②③ 时模型必须能看到「我这条本来要发什么」——只看链内历史，
// 会把「我这条里有新结论」误判成 `close`（历史里确实没见过这句）。

/// 待发正文被截断时的标注（与链内正文的截断标注分列，便于排查归因）
const PENDING_TRUNCATION_SUFFIX: &str = "…（待发正文已截断）";

/// 待发正文的切分预算（**同一池里留位**，不新增预算池）
///
/// `min(输入 × 15%, 正文池 / 4)`：标准容量下由前者绑定（= 600，与改造前一致），
/// 小容量下由后者收敛（不把链内证据挤掉）。
pub(crate) fn pending_token_budget(input_budget: usize, body_budget: usize) -> usize {
    context::pending_reply_tokens(input_budget, body_budget)
}

/// 判据（prompt 存调用方，服务端不持有）
///
/// **默认落在 `close`（仅通知）**：它代价最低——Final 仍作为知会送达对端，
/// 信息不丢、链深也不再增长；判不准时选它，最坏结果只是对端需要自己重开新链。
/// `continue` / `silent` 都要改变更贵的下游形态，必须真有把握才选。
///
/// 末段「省略标注 ⇒ 看不出就选 close」是本场景的保守方向：看不到中段不等于
/// 没有新信息，但也不能靠猜测继续加深链。
const INSTRUCTIONS: &str = "判断协作方刚发来的消息有没有新的实质信息、本轮还需不需要自动回发；\
看链内消息、链起点（`## 链起点`）、最近对话（`## 最近对话`）与待发消息（`## 待发消息`）。\n\
先看 `## 待发消息` —— 它是本条**本来就要发出去**的正文，属判据的一部分：\
它有对方需要知道的新内容（新结论 / 新数据 / 待办 / 问题）⇒ 选 continue；\
只有它本身就是对方已经知道的纯确认 / 纯致谢，才可能选 close / silent。\n\
选 continue：带来了上文没有的新事实 / 数据 / 结论 / 问题 / 待办，\
或提出需要答复的问题、需要确认的决策点、需要执行的下一步。\n\
选 close：只是确认 / 致谢 / 收讫 / 复述（「收到」「明白」「已完成」），\
或结论已在上文交付、本条只是重复总结，或同一层意思反复换措辞；\
**拿不准时也选 close** —— 它仍会把本条作为知会送达对端，信息不丢。\n\
选 silent：本条与本轮回复里都没有对方需要知道的内容（纯确认 / 纯致谢 / 纯重复），\
连「到此为止」都不必告诉对方。\n\
state 里的 window_depth（当前链深度）与 fatigue（疲劳值）**仅作辅助参考**：\n\
主判据是这条消息有没有新的实质信息量，不得仅因为这两个数字偏大就选 close。\n\
链内区块若出现「中间 N 条已省略」，不得把「看不到」当成「没有新信息」：\
无法确认时选 close，由对端决定是否重开新链。";

/// 三个选项（id → 判据描述）
///
/// `continue` / `close` 的 id 与语义**零改动**（旧轨迹照旧可解读），只新增
/// `silent`。`close` 与 `silent` 的判据刻意写成对照对：「对方是否需要知道
/// 『到此为止』」——这是模型唯一可判的差别，也是本改动最大的风险面
/// （两者区分度靠这一句，而不是靠 confidence）。
const OPTIONS: [(&str, &str); 3] = [
    (
        OPTION_CONTINUE,
        "继续回发：带来上文没有的新事实 / 结论 / 问题 / 待办；只有明确有新信息才选",
    ),
    (
        OPTION_CLOSE,
        "不再自动回发，但把本条作为知会送达对方：只是确认 / 致谢 / 收讫 / 复述，\
或同一层意思反复换措辞；拿不准时也选它",
    ),
    (
        OPTION_SILENT,
        "连「到此为止」都不必告诉对方：本条与本轮回复都没有对方需要知道的内容（纯确认 / 纯致谢 / 纯重复）",
    ),
];

/// 题面（instructions + 三个选项描述）的实测 token
///
/// 与协议层 `questions` 同源（同一份 `INSTRUCTIONS` / `OPTIONS` 常量），不另算一份
/// 「大概多少」。题面是常量，@4000 实测 **573 token**，已超过 `share::FRAME_BP`
/// 在标准容量下的 500 token 预留 —— 这不是估算误差，是比例预留装不下常量题面。
fn question_frame_tokens() -> usize {
    estimate_tokens(INSTRUCTIONS)
        + OPTIONS
            .iter()
            .map(|(id, desc)| estimate_tokens(id) + estimate_tokens(desc))
            .sum::<usize>()
}

/// 本场景 state 正文的**实际上限**：比例池与「容量 − 题面实测」取小
///
/// 六场景由 `render_within` 统一裁到 `body_budget_tokens`（= 容量 − 骨架份额）；
/// 本场景不接它，就必须自己保证「state + 题面 ≤ 容量」。题面小于骨架份额时
/// `min` 取比例池 ⇒ 既有分账逐字节不变；题面超出骨架份额时（小容量 / 常量题面
/// 偏长）按实测收紧，只在本场景少给一点链内正文，不动共用比例表、不波及六场景。
pub(crate) fn state_pool_tokens(input_budget: usize) -> usize {
    body_token_budget(input_budget).min(input_budget.saturating_sub(question_frame_tokens()))
}

/// 链内正文的可用预算（**总账收口的唯一出口**）
///
/// 三层扣减全在同一个池：① `state_pool_tokens` 按题面实测收紧；② 固定块实测
/// 占用；③ 待发正文实测占用。生产与护栏测试只认这一个函数，避免两处各算一遍
/// 之后口径漂移。
///
/// ⚠️ 收紧后**连固定块都装不下**的极小容量，保持改造前的分账「比例池 − 待发实测」：
/// 该档超窗是 provider 配置问题（题面本身就是常量 573 token），收紧只会把链内
/// 证据裁没，还会推翻「`body_budget == 0` 才不问 ⇒ 小容量照常问」的结构性判据。
/// 判据用「池 ≤ 固定块实测」而不是「容量 ≤ 题面」：后者会在刚过题面长度处留下
/// 一段「容量稍大反而不问」的非单调断崖。
pub(crate) fn chain_budget_tokens(
    brain: &Brain,
    origin_rendered: &str,
    facts: &ChainPressureFacts,
    input_budget: usize,
    pending_tokens: usize,
) -> usize {
    let fixed = fixed_sections_tokens(brain, origin_rendered, facts);
    let pool = state_pool_tokens(input_budget);
    if pool <= fixed {
        return body_token_budget(input_budget).saturating_sub(pending_tokens);
    }
    pool.saturating_sub(fixed).saturating_sub(pending_tokens)
}

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
    /// 发送方显示名（解析失败为 `None`，渲染时退回 id）
    ///
    /// 只有 id 时，模型很难分清「我方」与「对端」谁在复述谁；名字是零成本的
    /// 消歧线索（解析发生在调用方，本模块不做任何查询）。
    pub from_name: Option<String>,
    /// 消息类型的中文短标签（文本消息 / 工具执行结果 / …）
    pub type_label: &'static str,
    /// 正文（调用方原样给，本模块负责按预算截断）
    pub content: String,
    /// 创建时间戳（毫秒）；不参与渲染，仅供调用方/日志排序核对
    pub created_at: i64,
}

/// 链起点：沿 `reply_to_id` 回溯到第一条**非 Agent** 消息（通常是用户原话）
///
/// 追溯在 `User/System` 处停止，那条正文此前被整段丢掉，导致小脑只看到
/// 「双方在聊什么」，看不到「最初要解决什么」。它本来就在回溯手里，带上它
/// 不增加任何查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChainOrigin {
    /// 来源角色标签（`用户消息` / `系统消息`）
    pub role_label: &'static str,
    /// 发送方 ID
    pub from_id: String,
    /// 正文（本模块按 snippet 预算截断）
    pub content: String,
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
    let who = match &message.from_name {
        Some(name) => format!("{name}({})", message.from_id),
        None => message.from_id.clone(),
    };
    format!(
        "[#{}] {} {}: {}",
        index + 1,
        who,
        message.type_label,
        message.content.trim()
    )
}

/// 链内正文的截断标注（实现见 `compaction::truncate_to_tokens`，
/// 小脑 state 裁剪共用同一实现、只差这段文案）
const BODY_TRUNCATION_SUFFIX: &str = "…（正文已截断）";

/// 组装链内输入：保链头 [`HEAD_KEEP`] 条 + 最近 N 条，双阈取小，中间显式标注省略
///
/// 为什么保链头：链头决定「这整条链在谈什么事」，丢掉它，模型只能看到
/// 尾部几句确认语，必然误判 close。一条往往只写了开场白，故保 3 条才够
/// 看出议题与分工。为什么保尾部：新消息是否带来信息，主要由最近几跳的语义差决定。
pub(crate) fn render_chain(messages: &[ChainMessage], budget: usize) -> ChainInput {
    if messages.is_empty() || budget == 0 {
        return ChainInput {
            rendered: String::new(),
            included: 0,
            omitted: 0,
        };
    }
    let total = messages.len();
    // 条数硬上限（双阈取小的「条数」一侧）：超限时保链头 + 最近 N-head 条，
    // 更早的中间段既不渲染也不占 token 预算。
    let head_count = HEAD_KEEP.min(total).min(MAX_CHAIN_MESSAGES);
    let dropped = total.saturating_sub(MAX_CHAIN_MESSAGES);
    let tail_start = if dropped == 0 {
        head_count
    } else {
        total - (MAX_CHAIN_MESSAGES - head_count)
    };
    // 省略标注本身也占预算，先预留最坏情况（全部中间条被省略）
    let reserve = estimate_tokens(&omission_line(total, total));

    // 链头逐条渲染：整条放得下就整条放，放不下就截断这一条后停止
    // （保住「链从哪来」这一端，并给省略标注留出空间）。
    let mut heads: Vec<String> = Vec::new();
    let mut used = 0usize;
    for (index, message) in messages.iter().take(head_count).enumerate() {
        let available = budget.saturating_sub(used).saturating_sub(reserve);
        if available == 0 && !heads.is_empty() {
            break;
        }
        let line = render_one(index, message);
        let fits = estimate_tokens(&line) <= available;
        let rendered = if fits {
            line
        } else {
            truncate_to_tokens(&line, available, BODY_TRUNCATION_SUFFIX)
        };
        used += estimate_tokens(&rendered);
        heads.push(rendered);
        if !fits {
            break;
        }
    }

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

    let included = heads.len() + tail.len();
    let omitted = total - included;
    let mut rendered = heads.join("\n");
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
/// `会话身份` → `当前场景` → `最近对话` → `链起点` → `链内消息` → `待发消息` → `协作压力`
///
/// 两处刻意的「不」：
/// - **不用 `with_user_message`**：本场景的输入是 N 条链内历史正文，不是「当前用户
///   消息」；那个标题会把多轮历史误导成一条待判正文。
/// - **链起点是独立块而不是塞进链内消息**：它不在 `reply_to_id` 的 Agent 往返里，
///   混进去会把「用户最初要什么」伪装成一次 Agent 回发。
///
/// 也**不做组织器侧容量自截**（`render_within`）：链内正文已由 [`render_chain`] 按
/// 实测余额自截；再叠一层组织器的「容量 − 骨架比例」**尾块整砍**，会在中间容量带
/// 把 `## 链内消息` 这块主要证据整块砍掉 —— 比不截断更坏（实测见 `chain_reply_tests`）。
///
/// ⚠️ 但「不接 `render_within`」不等于「不做总账」：`body_budget_tokens` 在六场景
/// 是**整个 state 正文**的预算，本场景必须由 [`fixed_sections_tokens`] 把其余块的
/// 实测占用从同一个池扣掉，余额才交给链内 / 待发两块 —— 否则「87.5% 全给两块 +
/// 最近对话 12% + 身份/场景/协作压力 + 题面 12.5%」会越过 100%。容量是否够问的
/// 判据仍是「`body_token_budget == 0` ⇒ 不问」（⑤），不是「截到装得下」。
fn state_bundle(
    brain: &Brain,
    origin_rendered: &str,
    chain_rendered: &str,
    pending_rendered: &str,
    facts: &ChainPressureFacts,
) -> context::ContextBundle {
    let mut bundle = context::ContextBundle::new()
        .with_identity(&brain.agent_id, &brain.agent_name)
        .with_scene(SCENE)
        // 最近对话（`Brain.memories` 尾部 N 条）：AMan 2026-11-11 定为正式判据输入。
        // 本场景单列 8 条（见 `CHAIN_RECENT_DIALOGUE_LIMIT`），其余场景仍是 3。
        .with_recent_dialogue(
            &brain.memories,
            CHAIN_RECENT_DIALOGUE_LIMIT,
            context::dialogue_snippet_tokens(brain),
        );
    // 链起点（用户 / 系统原话）：空正文整块跳过，不留裸标题
    bundle.push("链起点", origin_rendered.to_string());
    // 空正文（一条链内消息都没带上）时 `push` 整块跳过，不留 `## 链内消息` 裸标题
    bundle.push("链内消息", chain_rendered.to_string());
    // 待发消息：本条**默认准备发送**的正文（判 ①②③ 的第三个输入面）。
    // 空待发正文同样整块跳过 —— 不给模型一个空壳标题去猜。
    bundle.push("待发消息", pending_rendered.to_string());
    bundle.push("协作压力", pressure_body(facts));
    bundle
}

/// 链内 / 待发两块之外的固定 state 开销（token，**实测口径**）
///
/// `share::TOTAL_BP` 的编译期断言只钉「骨架 + 待发 + 正文 = 100%」，钉不住本场景
/// 的语义分叉：六场景由 `render_within` 把**所有块**压到 `body_budget_tokens` 以内，
/// 本场景却把整个正文池当成了「链内 + 待发」两块的预算。故这里按实测把身份 /
/// 场景 / 最近对话 / 链起点 / 协作压力（含 `## 标题` 与 `\n\n` 分隔符）从同一个
/// 池扣掉。
///
/// 占位块用 **CJK 单字**：`estimate_tokens` 对 CJK 是严格 1 字 / 1 token 的线性
/// 计数，减去两个占位后即为固定块的精确开销；用拉丁字符会踩「4 字符 1 token、
/// 向上取整」的分界，拼接后可能少算 1 token。最终态若有块因空正文被跳过，本估算
/// 只会偏高 ⇒ 预算更保守，不会越界。容量未配时 `body_budget` 极大，扣减不改变
/// 「不裁」的既有行为。
fn fixed_sections_tokens(
    brain: &Brain,
    origin_rendered: &str,
    facts: &ChainPressureFacts,
) -> usize {
    const PLACEHOLDER: &str = "占";
    let rendered = state_bundle(brain, origin_rendered, PLACEHOLDER, PLACEHOLDER, facts).render();
    estimate_tokens(&rendered).saturating_sub(2 * estimate_tokens(PLACEHOLDER))
}

/// 本场景的 state 装配（单出口：`{"context": "<分块正文>"}`）
fn state_metrics(
    brain: &Brain,
    origin_rendered: &str,
    chain_rendered: &str,
    pending_rendered: &str,
    facts: &ChainPressureFacts,
) -> Metrics {
    Metrics::new().with(
        context::STATE_KEY,
        state_bundle(
            brain,
            origin_rendered,
            chain_rendered,
            pending_rendered,
            facts,
        )
        .render(),
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
    // 代价越大、门槛越高：close 直接采纳，silent 要 0.8，continue 要 0.9。
    .with_option_min_confidence(OPTION_CLOSE, MIN_CONFIDENCE_CLOSE)
    .with_option_min_confidence(OPTION_SILENT, MIN_CONFIDENCE_SILENT)
    .with_option_min_confidence(OPTION_CONTINUE, MIN_CONFIDENCE_CONTINUE)
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

/// 生产入口：判一次「还要不要继续回发」
///
/// 返回 `None` 只代表**小脑没参与**（未装配 / 未启用 / 调用失败 / 超时 / 配额 /
/// 空输入），此时调用方走原静态判定。**「答了但不可用」不再算 `None`**：
/// 按最保守选项默认 `close`（见 [`DECISION_CLOSE_DEFAULT`]）。
///
/// `brain` 刻意收 `Option<&Brain>`：调用方直接传 `agent.brain.as_ref()`，
/// 缺失即降级（本门绝不阻断主流程，也不 panic）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn judge_chain_reply(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: Option<&Brain>,
    origin: Option<&ChainOrigin>,
    messages: &[ChainMessage],
    facts: &ChainPressureFacts,
    pending_reply: &str,
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

    // ④ 输入预算 = 小脑模型容量（**两层**：推荐长度是裁剪基准，最大长度是唯一硬顶）
    //
    // 取值收敛到 `context::context_capacity` 单一入口（= min(推荐, 最大)），
    // 不在本场景再写一遍「推荐值 > max × 60%」的链——同一语义两处实现必然漂移。
    // 超过**推荐长度**的内容走裁剪（⑤.5 / ⑥ 的 truncate），不判失败；只有预算被
    // 夹到 0（连判据骨架都装不下）才是 ⑤ 的收口条件。
    let capacity_tokens = context::context_capacity(brain);
    let input_budget = input_token_budget(capacity_tokens);
    let body_budget = body_token_budget(input_budget);

    // ⑤ 正文预算为 0：不问，也不构造退化请求
    //
    // **本场景唯一的「不发」判据**（AMan 2026-11-11 口径）：超过推荐长度不算超限，
    // 一律走裁剪后照常发送；只有预算被夹到 0（连骨架都装不下，通常意味着 provider
    // 连 `max_context_length` 都没配出可用值）才收口。命中时 `degraded` 仍记
    // `capacity_too_small` —— 值域不变，避免历史轨迹失去可解读性。
    //
    // 结构性判据（不是「最低可用 token 数」这种绝对值）：预算 0 ⇒ 正文必然空，
    // 发出去只会是一句「…（正文已截断）」，请求发得出去、账单照付、结论纯噪声。
    // 显式变成「不问」比「问了也白问」诚实。
    if body_budget == 0 {
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_FALLBACK,
            degraded_reason = DEGRADED_CAPACITY,
            scene = SCENE,
            capacity_tokens = capacity_tokens.unwrap_or(0),
            input_budget = input_budget,
            body_budget = body_budget,
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

    // ⑤.4 链起点（用户 / 系统原话）：与摘要同档截断（snippet 预算）
    //
    // 它是「这条链最初要解决什么」的唯一来源，且调用方回溯时本来就读到了；
    // 容量未配时 `dialogue_snippet_tokens` 为 `None` ⇒ 不截断（与其他块同口径）。
    let origin_rendered = origin
        .map(|o| {
            let text = o.content.trim();
            let text = match context::dialogue_snippet_tokens(brain) {
                Some(budget) => truncate_to_tokens(text, budget, BODY_TRUNCATION_SUFFIX),
                None => text.to_string(),
            };
            format!("{}（{}）: {}", o.role_label, o.from_id, text)
        })
        .unwrap_or_default();

    // ⑤.5 待发正文（本条默认准备发送的消息）—— AMan 2026-11-11 新增的判定输入
    //
    // 分账规则（**不新开预算池**）：
    // - 同一池：`state_pool` 是本场景 state 正文的上限（比例池与「容量 − 题面实测」
    //   取小，见 [`state_pool_tokens`]），先给它切一小块
    //   （`pending_token_budget` = min(输入×15%, body/4)），再按**实测占用**扣减；
    //   固定块与待发正文都从同一池扣除后，余额才给链内正文 ⇒ state + 题面仍
    //   落在容量内，且待发正文短时不会白占固定预留。
    // - 两重比例约束（输入 15% 上限 + 正文池 1/4）已保证待发正文挤不掉链内证据，
    //   故不再减一个绝对值下限：预算极小时它自然收敛到 body/4，为 0 就不带
    //   （块缺席），不留一句「已截断」的空壳去污染判据。
    let pending_budget = pending_token_budget(input_budget, body_budget);
    let pending = if pending_budget == 0 {
        String::new()
    } else {
        truncate_to_tokens(
            pending_reply.trim(),
            pending_budget,
            PENDING_TRUNCATION_SUFFIX,
        )
    };
    // ⚠️ 总账收口：本场景不接 `render_within`，就得自己保证「整个 state 正文 ≤
    // state_pool」（六场景由组织器保证）。`chain_budget_tokens` 单一出口负责
    // 「题面实测收紧 + 固定块实测 + 待发实测」三层扣减；少了它，12% 的最近对话
    // + 固定块 + 573 token 题面会叠在 87.5% 之外，推荐值≈最大长度的 provider
    // 会被顶出窗口。极小容量保持改造前分账（见该函数 doc）。
    let chain_budget = chain_budget_tokens(
        brain,
        &origin_rendered,
        facts,
        input_budget,
        estimate_tokens(&pending),
    );

    // ⑥ 链内一条消息都拉不到：不调小脑、不占配额
    let input = render_chain(messages, chain_budget);
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
    let metrics = state_metrics(brain, &origin_rendered, &input.rendered, &pending, facts);

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

    // 阶段三：答案注入事实桶，按**该选项自己的门槛**判定是否采纳
    //
    // 与旧的单条全局门槛不同：`close` 代价最低（仍把 Final 作为知会送达），
    // 任何时候都采纳；`silent` / `continue` 分别要 0.8 / 0.9。任何一个不达标
    // 都落到 `close` —— 「不知道该怎么回，就仅通知」，这是代价最小的姿态。
    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let overrode_static_wall = facts.window_depth as u64 >= facts.hard_window_depth;
    let (pick, reason) = match metrics.decision(&policy.primary_qid()) {
        Some(DecisionValue::Pick {
            chosen, confidence, ..
        }) => match policy.option_threshold(chosen) {
            Some(threshold) if *confidence >= threshold => (
                Some(PickOne {
                    option: chosen.clone(),
                    confidence: *confidence,
                }),
                None,
            ),
            // 答了但没达到该选项门槛
            Some(_) => (None, Some(DEGRADED_LOW_CONFIDENCE)),
            // 模型编造的选项：不在选项集内 = 没有结论
            None => (None, Some(DEGRADED_NO_ANSWER)),
        },
        // 没答 / 题型不符
        _ => (None, Some(DEGRADED_NO_ANSWER)),
    };
    let Some(pick) = pick else {
        let reason = reason.unwrap_or(DEGRADED_NO_ANSWER);
        log_info!(
            ctx,
            "chain_reply",
            decision = DECISION_CLOSE_DEFAULT,
            degraded_reason = reason,
            scene = SCENE,
            latency_ms = latency_ms,
            window_depth = facts.window_depth,
            hard_window_depth = facts.hard_window_depth,
            overrode_static_wall = overrode_static_wall,
            "chain reply gate has no usable answer, default to close (notify only)"
        );
        record_cerebellum(
            trace_id,
            trace(
                DECISION_CLOSE_DEFAULT,
                None,
                Some(reason),
                !overrode_static_wall,
                Some(latency_ms),
            ),
        );
        return Some(ChainReplyVerdict::Close { confidence: None });
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
    origin: Option<&ChainOrigin>,
    messages: &[ChainMessage],
    facts: &ChainPressureFacts,
    pending_reply: &str,
    trace_id: &str,
) -> Option<ChainReplyVerdict> {
    judge_chain_reply(
        ctx,
        dal(),
        brain,
        origin,
        messages,
        facts,
        pending_reply,
        trace_id,
    )
    .await
}

#[cfg(test)]
#[path = "chain_reply_tests.rs"]
mod tests;
