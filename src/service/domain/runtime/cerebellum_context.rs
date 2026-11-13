//! 小脑决策点的上下文组织器（续令第 1 点 · 笔④）
//!
//! # 为什么有它
//! 三个决策点（`intent_gate` / `wake_gate` / `summary_gate`）此前各自逐字重写同一段
//! state 装配：`Metrics::new().with("agent_id",…).with("agent_name",…)
//! .with("last_user_message",…)`。同类上下文重复拼装只是表层问题；真正的缺口是
//! `Brain.memories` 明明挂在结构上，却没有一个场景带上它 —— 要判「这是不是多步任务
//! 的中间句」「会不会聊过就忘」，却看不到任何上文。
//!
//! # 形态（仿 prompt builder，但不抽公共容器）
//! 一组消费型区块构造器 + 单一 [`ContextBundle::render`] 出口：
//! - 通用层（本模块）只做「收集 + 渲染」，**不认识任何 scene**；
//! - scene 特化留在各场景侧（决定「要哪几块」）；
//! - 用不上的块直接不 push（等价于 prompt builder 里「用不上的区块走默认空实现」）。
//!
//! 不抽公共容器：prompt builder 绑死大脑域实体类型且产出 `ChatMessage[]`，本模块
//! 只产 `DecisionRequest.state` 里的一个字符串，两者共享的只有四条判据（单一装配点
//! / 只追加不改写 / 确定性排序 / 空块跳过），不是代码。
//!
//! # 边界（不要越界）
//! 本模块**只碰 `DecisionRequest.state`，不碰问题与选项**：`QuestionSpec`
//! （qid / instructions / options / levels）仍归各场景与形态类。往这里塞 options
//! 会把「选项只放关键 id + 摘要」那条改造的范围吃掉。
//!
//! # state 形态
//! 出口经 [`STATE_KEY`] 落成 `{"context": "<分块正文>"}`：`collect_state` 一行不改，
//! DAL 与协议零改动（`CerebellumRequest.state` 本就是 `Value`，wire format doc 明写
//! 允许字符串 / 对象 / 数组）。
//!
//! # 两层预算（AMan 2026-11-11 口径：超推荐不收口，硬顶才收口）
//! - **推荐上下文长度**＝裁剪基准（软目标）：内容超了就按比例裁，**不判失败、
//!   不降级、不拦截**，请求照常发出（[`ContextBundle::render_within`] 的产物
//!   仍是合法分块正文）；
//! - **最大上下文长度**＝唯一硬顶：只有连它都装不下才收口，且本次预算永不越过它
//!   （[`context_capacity`] 用 `min(推荐, 最大)` 夹住）。
//!
//! 两层的分工必须写死在取值入口，不许各调用点自己判：配置里推荐值大于最大长度时
//! 按最大长度封顶（那不是「超过推荐」，是真超限 —— provider 会直接拒请求）。
//!
//! # 容量自截（已裁定：组织器自截，`dal/brain.rs` 一行不动）
//! `dal/brain.rs::shrink_state_to_capacity` 只认 `last_user_message` 键，state 改成
//! `context` 单键后那条兜底不再命中 —— 这属**已发生的等价性缺口**，不是可选优化。
//! 处置已裁定为「组织器自截」：[`ContextBundle::render_within`] 按模型容量从尾部
//! 整块砍（标注省略块数 N），必要时对末块正文截尾，**保证不出现无 body 的裸
//! `## 标题`**。不改 dal 侧的两条理由：块边界（`## 标题` + 正文）是本模块的输出格式，
//! dal 侧要知道它就得硬编码 domain 约定，而 `dal` → `domain` 是反向依赖；且 dal 那个
//! 兜底点是 13 个 scene 共用的，改它要评估全部场景、回滚面也更大。

use crate::models::brain::Brain;
use crate::models::cortex_types::ChatMessage;
use crate::models::memory::{Memory, MemoryPo};

use super::compaction::{estimate_tokens, truncate_to_tokens};

/// state 里承载分块正文的键名
///
/// 单出口：所有接入场景都经 [`STATE_KEY`] 落到 state，不得各写字符串字面量。
pub(crate) const STATE_KEY: &str = "context";

/// 历史对话块默认条数（尾部取最近 N 条）
pub(crate) const RECENT_DIALOGUE_LIMIT: usize = 3;

// # 输入预算的比例分账（**唯一事实源**）
//
// AMan 2026-11-11 口径：「这一部分数据占几成，那一部分几成，只要最终不超过
// 最大长度就可以」。故各块占比集中在本节，消费方只做「占比 × 预算」的乘法，
// **不得再各自拍固定 token 数**。唯一硬顶 = 各块之和 ≤ [`share::BASIS`]（100%）。

/// 小脑一次判据的输入预算分账（basis points；1 bp = 0.01%）
///
/// 取值的来源是**现状实测反推**，不是新拍：@输入 4000 token 时
/// `FRAME_BP` ⇒ 500（= 原 `FRAME_OVERHEAD_RESERVE`）、
/// `PENDING_REPLY_BP` ⇒ 600（= 原 `PENDING_REPLY_TOKEN_CAP`）、
/// `BODY_BP` ⇒ 2900（= 原 `4000 − 500 − 600`）——三项逐位复现改造前实值。
pub(crate) mod share {
    /// 判据骨架：instructions + options + 题面 + 协议包装
    pub(crate) const FRAME_BP: u32 = 1_250; // 12.5%
    /// 待发消息（本条默认准备发送的正文）
    pub(crate) const PENDING_REPLY_BP: u32 = 1_500; // 15%
    /// 正文池（链内消息 / state 正文）
    pub(crate) const BODY_BP: u32 = 7_250; // 72.5%

    /// 单条摘要片段（历史对话 / 检索候选 / 索引行）的正文份额
    ///
    /// **块内派生，不进 [`TOTAL_BP`]**：它切的是各块正文里的一行，不是独立预算池
    /// ——算进总和会把「同一块数据分几段写」重复计费。@输入 4000 ⇒ 160 token，
    /// 与改造前「160 字符」（CJK 最坏情况）逐位一致。
    pub(crate) const SNIPPET_BP: u32 = 400; // 4%

    /// 渠道入站判定：群聊历史池（36%）
    pub(crate) const INBOUND_HISTORY_BP: u32 = 3_600;
    /// 渠道入站判定：正文检索到的相关记忆（14.4%）
    pub(crate) const INBOUND_SEARCHED_MEMORY_BP: u32 = 1_440;
    /// 渠道入站判定：Agent 最近短期记忆（14.4%）
    pub(crate) const INBOUND_RECENT_MEMORY_BP: u32 = 1_440;
    /// 渠道入站判定：本次消息（7.2%）
    pub(crate) const INBOUND_CURRENT_BP: u32 = 720;
    /// 渠道入站判定的上下文总和（必须 ≤ 正文池 `BODY_BP`）
    pub(crate) const INBOUND_TOTAL_BP: u32 = INBOUND_HISTORY_BP
        + INBOUND_SEARCHED_MEMORY_BP
        + INBOUND_RECENT_MEMORY_BP
        + INBOUND_CURRENT_BP;

    /// 基准：10_000 bp = 100%
    pub(crate) const BASIS: u32 = 10_000;
    /// 占比总和（自校验锚点，见 [`validate_share_sum`]）
    pub(crate) const TOTAL_BP: u32 = FRAME_BP + PENDING_REPLY_BP + BODY_BP;
}

// 入站判定是正文池的**子分配**，不得越过 `BODY_BP`（否则会挤占判据骨架）
const _: () = assert!(share::INBOUND_TOTAL_BP <= share::BODY_BP);

// 骨架预留不再有绝对上限：AMan 2026-11-11 终裁删掉 500。骨架 = 输入 × `share::FRAME_BP`
// 纯比例 —— 输入大时随容量线性增长（比例分账的本意就是「不超过最大长度即可」，
// 不该再叠一层与容量无关的固定封顶）。历史定标（实测骨架 ≈412）只用于确认
// `FRAME_BP` = 12.5% 在标准容量下与旧值逐位一致，不再作为写死的上界。

// 正文预算不再有「最小可判 token 数」这个绝对值：AMan 2026-11-11 终裁删掉 100。
// 判据换成结构性的「预算 == 0 ⇒ 不截」，`render_within` 与 `chain_reply` 各自
// 用同一句 `== 0` 判断（零魔法值）。

/// 待发消息占正文池的份额分母（最多 1/4）
const PENDING_SHARE_DIVISOR: usize = 4;

/// 占比之和校验：> 100% 即超限（**唯一硬顶**，不允许静默超限）
pub(crate) fn validate_share_sum(total_bp: u32) -> Result<(), String> {
    if total_bp > share::BASIS {
        return Err(format!(
            "cerebellum budget shares exceed the hard ceiling: {total_bp} bp > {} bp",
            share::BASIS
        ));
    }
    Ok(())
}

/// 按占比切分预算（1 bp = 0.01%，向下取整）
pub(crate) fn bp_of(input_budget: usize, bp: u32) -> usize {
    input_budget.saturating_mul(bp as usize) / share::BASIS as usize
}

/// 骨架预留 = 输入 × `FRAME_BP`（纯比例，无绝对上限）
pub(crate) fn frame_tokens(input_budget: usize) -> usize {
    debug_assert!(validate_share_sum(share::TOTAL_BP).is_ok());
    bp_of(input_budget, share::FRAME_BP)
}

/// 正文池 = 输入 − 骨架（不足归零，不下溢）
pub(crate) fn body_budget_tokens(input_budget: usize) -> usize {
    input_budget.saturating_sub(frame_tokens(input_budget))
}

/// 待发消息预算 = min(输入 × `PENDING_REPLY_BP`, 正文池 / 4)
///
/// 同一池分账（**不新增预算池**）：上限按输入的比例、再被正文池的四分之一夹住，
/// 小容量下自然收敛到 1/4，不会把链内证据挤掉。
pub(crate) fn pending_reply_tokens(input_budget: usize, body_budget: usize) -> usize {
    bp_of(input_budget, share::PENDING_REPLY_BP).min(body_budget / PENDING_SHARE_DIVISOR)
}

/// 整块被砍时的标注前缀（后接「N 块）」，N = 注册块数 − 保留块数）
const OMISSION_NOTE_PREFIX: &str = "…（上下文已按模型容量截断，省略 ";

/// 只有一块、且该块正文被截尾时的标注（此时没有「被砍掉的块」，写块数会说谎）
const TAIL_TRUNCATION_NOTE: &str = "…（正文已按模型容量截断）";

/// 一个上下文区块：标题 + 正文
pub(crate) struct ContextSection {
    title: &'static str,
    body: String,
}

/// 上下文集合：只追加、不改写，按注册序渲染
#[derive(Default)]
pub(crate) struct ContextBundle {
    sections: Vec<ContextSection>,
}

impl ContextBundle {
    /// 空集合
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 会话身份块（调用方从 `Brain` 取实际取值传入）
    pub(crate) fn with_identity(mut self, agent_id: &str, agent_name: &str) -> Self {
        // 逐字段判空：两个都空 = 没拿到身份 ⇒ 整块不 push（不留 `agent_name: ` 空占位）
        let mut lines = Vec::new();
        if !agent_id.trim().is_empty() {
            lines.push(format!("agent_id: {agent_id}"));
        }
        if !agent_name.trim().is_empty() {
            lines.push(format!("agent_name: {agent_name}"));
        }
        self.push("会话身份", lines.join("\n"));
        self
    }

    /// 当前场景块（让模型知道自己在判哪个决策点）
    pub(crate) fn with_scene(mut self, scene: &str) -> Self {
        self.push("当前场景", scene.to_string());
        self
    }

    /// 当前用户消息块（正文放原文，供模型读；选项里才只放 id + 摘要）
    pub(crate) fn with_user_message(mut self, text: &str) -> Self {
        self.push("当前用户消息", text.to_string());
        self
    }

    /// 最近若干条历史块（尾部 N 条、时间正序保留；一条都取不到就整块不 push）
    ///
    /// `snippet_tokens` = 单条摘要的 token 预算（见 [`snippet_budget`]）；
    /// `None` ⇒ 不截断（模型容量未配置时不猜预算，与 `input_token_budget(None)`
    /// 的「容量未配不裁」同口径）。
    pub(crate) fn with_recent_dialogue(
        mut self,
        memories: &[Memory],
        limit: usize,
        snippet_tokens: Option<usize>,
    ) -> Self {
        let start = memories.len().saturating_sub(limit);
        let mut lines = Vec::new();
        for memory in &memories[start..] {
            let line = dialogue_line(memory, snippet_tokens);
            if !line.is_empty() {
                lines.push(format!("- {line}"));
            }
        }
        self.push("最近对话", lines.join("\n"));
        self
    }

    /// 追加一个区块
    ///
    /// 正文 `trim()` 后为空则**整块跳过**：绝不产出「标题 + 空 body」——
    /// 空标题对模型是噪音，对排查是误导（看起来带了上下文，其实什么都没带）。
    pub(crate) fn push(&mut self, title: &'static str, body: String) {
        if body.trim().is_empty() {
            return;
        }
        self.sections.push(ContextSection { title, body });
    }

    /// 唯一出口：按注册序渲染成 `## <标题>` 分块正文
    ///
    /// 注册序即渲染序（`Vec` 而非 `HashMap`）：同一组输入必然逐字节同输出，
    /// 前缀稳定性可被断言，也不给「两次调用得出不同上下文」留口子。
    pub(crate) fn render(&self) -> String {
        self.sections
            .iter()
            .map(|section| format!("## {}\n{}", section.title, section.body))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// 按模型容量渲染（容量自截；`None` 或预算不足 ⇒ 逐字节等于 [`Self::render`]）
    ///
    /// `capacity_tokens` 由调用方从
    /// `ModelProviderConfig::recommended_context_length_tokens()` 取（与 `dal` 侧兜底同源）。
    /// 不做截断的两档刻意「逐字节等于 `render()`」：静默改语义比不截更糟。
    pub(crate) fn render_within(&self, capacity_tokens: Option<u64>) -> String {
        let Some(capacity) = capacity_tokens else {
            return self.render();
        };
        // 预算 = 容量 × 正文占比（比例表唯一入口，不再减固定预留）
        let budget = body_budget_tokens(usize::try_from(capacity).unwrap_or(usize::MAX));
        // 结构性判据（不是「最低可用 token 数」）：预算 0 ⇒ 不截（逐字节 == render()）
        if budget == 0 {
            return self.render();
        }
        self.render_within_tokens(budget)
    }

    /// 真正的按预算渲染（**私有**）
    ///
    /// 私有是为了让测试能构造极小预算走到「末块截尾」分支：经 [`Self::render_within`]
    /// 入口时预算恒 > 0（0 已被 `render_within` 早退拦下），到不了 0 边界。
    fn render_within_tokens(&self, budget: usize) -> String {
        let full = self.render();
        if estimate_tokens(&full) <= budget {
            return full;
        }
        let total = self.sections.len();
        // 从尾部整块砍：keep 递减，标注里写出被砍掉的块数 N = total − keep
        for keep in (1..total).rev() {
            let candidate = self.render_kept(keep, total - keep);
            if estimate_tokens(&candidate) <= budget {
                return candidate;
            }
        }
        // 连首块单独都超预算 ⇒ 保标题 + 正文截尾（标题恒有正文，绝不裸标题）
        let title = self.sections[0].title;
        let note = if total > 1 {
            omission_note(total - 1)
        } else {
            TAIL_TRUNCATION_NOTE.to_string()
        };
        let overhead = estimate_tokens(&format!("## {title}\n")) + estimate_tokens(&note) + 1; // 标注前那个换行
        let body_budget = budget.saturating_sub(overhead);
        let body = truncate_body_keep_visible(&self.sections[0].body, body_budget);
        format!("## {title}\n{body}\n{note}")
    }

    /// 保留前 `keep` 块并附省略标注（标注紧贴末块正文，不另起段落）
    fn render_kept(&self, keep: usize, omitted: usize) -> String {
        let mut rendered = self
            .sections
            .iter()
            .take(keep)
            .map(|section| format!("## {}\n{}", section.title, section.body))
            .collect::<Vec<_>>()
            .join("\n\n");
        rendered.push('\n');
        rendered.push_str(&omission_note(omitted));
        rendered
    }
}

/// 通用场景装配点：身份 + 场景 + 用户消息 + 最近对话，并按模型容量自截
///
/// 五个通用决策点（三 gate / entry_gate / entry_pool）走这里，不再各写一遍
/// `Metrics::new().with(...)` 拼装。`thinking_strength` 走
/// `thinking_strength::render_strength_context` **专用装配**：它要带节点现场
/// （消息序列 / 轮次 / 工具 / 配置原值），且必须把高价值块前置 —— 通用装配的
/// 块序（用户消息在中、最近对话在尾）会把这些新信息排到最先被砍的位置。
pub(crate) fn render_for(brain: &Brain, scene: &str, user_text: &str) -> String {
    ContextBundle::new()
        .with_identity(&brain.agent_id, &brain.agent_name)
        .with_scene(scene)
        .with_user_message(user_text)
        .with_recent_dialogue(
            &brain.memories,
            RECENT_DIALOGUE_LIMIT,
            dialogue_snippet_tokens(brain),
        )
        .render_within(context_capacity(brain))
}

// ==================== 节点输入要点（思考强度场景的通用收集器） ====================
//
// 本模块只**收集**，不认识任何 scene：从传入节点的 `ChatMessage[]` 里抽出
// 「规模 / 大脑已注入的背景区块 / 任务目标摘要 / 已发生的工具调用 / 最近动作」
// 五类要点，交给场景侧决定渲染顺序。
//
// 为什么必须提取而不是整段塞：生产里 compact / summary / settle 节点的最后一条
// User 消息其实是**整份拼装 Prompt**（实测 34K token），原样进小脑 state 只会把
// 判据稀释到看不见。提取后既有「目标 / 背景 / 规模」，又守住小脑的容量预算。

/// 背景区块标题最多列几个（再多只是噪音，且会把后面的要点挤出预算）
const MAX_SECTION_TITLES: usize = 12;
/// 工具名最多列几种
const MAX_TOOL_NAMES: usize = 8;
/// 最近动作取尾部几条
const RECENT_ACTIONS: usize = 3;
/// 单条最近动作摘要的字符上限
const ACTION_SNIPPET_CHARS: usize = 96;

/// 待抽取摘要的背景区块（按大脑 prompt 里的实际区块名）
const GOAL_SECTIONS: [&str; 3] = ["任务上下文", "项目上下文", "输入理解结果"];

/// 从节点消息序列里收集**结构化要点**（每行一条，供场景侧拼块）
///
/// `snippet_tokens` = 单条摘要的 token 预算；`None` ⇒ 不截（容量未配不猜预算，
/// 与 `render_within` 同口径）。
///
/// ⚠️ 只走 [`ChatMessage::to_summary_text`] 取最近动作：它已经处理了两条红线
/// —— 多模态只输出图像计数（base64 不外泄）、Reasoning 只输出摘要计数
/// （`encrypted_content` 不外泄）。这里不自己拼正文。
pub(crate) fn node_message_facts(
    messages: &[ChatMessage],
    snippet_tokens: Option<usize>,
) -> Vec<String> {
    if messages.is_empty() {
        return vec!["（本节点未带消息序列）".to_string()];
    }

    let mut roles = [0usize; 5]; // system / user / assistant / tool / reasoning
    let mut total_tokens = 0usize;
    let mut system_text = String::new();
    let mut tool_names: Vec<(String, usize)> = Vec::new();
    let mut tool_call_count = 0usize;
    let mut tool_result_tokens = 0usize;

    for message in messages {
        match message {
            ChatMessage::System { content } => {
                roles[0] += 1;
                total_tokens += estimate_tokens(content);
                // 大脑的 System 只有一条（人设 + 全部上下文区块）；多条时取首条，
                // 避免把重复前缀的 token 算两遍、也让标题去重有稳定来源。
                if system_text.is_empty() {
                    system_text.push_str(content);
                }
            }
            ChatMessage::User { content } => {
                roles[1] += 1;
                total_tokens += estimate_tokens(content);
            }
            ChatMessage::UserMultimodal { text, .. } => {
                roles[1] += 1;
                total_tokens += estimate_tokens(text);
            }
            ChatMessage::Assistant {
                content,
                tool_calls,
            } => {
                roles[2] += 1;
                if let Some(content) = content {
                    total_tokens += estimate_tokens(content);
                }
                if let Some(calls) = tool_calls {
                    tool_call_count += calls.len();
                    for call in calls {
                        if let Some((_, count)) =
                            tool_names.iter_mut().find(|(name, _)| *name == call.name)
                        {
                            *count += 1;
                        } else if tool_names.len() < MAX_TOOL_NAMES {
                            tool_names.push((call.name.clone(), 1));
                        }
                    }
                }
            }
            ChatMessage::Tool { content, .. } => {
                roles[3] += 1;
                let tokens = estimate_tokens(content);
                total_tokens += tokens;
                tool_result_tokens += tokens;
            }
            ChatMessage::Reasoning { .. } => roles[4] += 1,
        }
    }

    let mut lines = vec![format!(
        "消息序列：共 {} 条（system {} / user {} / assistant {} / tool {} / reasoning {}），估算约 {} token",
        messages.len(),
        roles[0],
        roles[1],
        roles[2],
        roles[3],
        roles[4],
        total_tokens
    )];

    let titles = collect_section_titles(&system_text);
    if !titles.is_empty() {
        lines.push(format!("大脑已注入的背景区块：{}", titles.join(" / ")));
    }
    for title in GOAL_SECTIONS {
        if let Some(excerpt) = section_excerpt(&system_text, title, snippet_tokens) {
            lines.push(format!("【{title}】摘要：{excerpt}"));
        }
    }
    if tool_call_count > 0 {
        let names = tool_names
            .iter()
            .map(|(name, count)| format!("{name}×{count}"))
            .collect::<Vec<_>>()
            .join("、");
        lines.push(format!(
            "已发起工具调用 {} 次（{}），工具结果 {} 条约 {} token",
            tool_call_count, names, roles[3], tool_result_tokens
        ));
    }

    // 尾部取 N 条 → 反转成时间正序：倒序展示会让「最近动作」读起来像从新到旧，
    // 与 `## 最近对话` 的时间方向相反，模型容易把因果关系看反。
    let mut recent: Vec<String> = messages
        .iter()
        .rev()
        .filter(|m| !matches!(m, ChatMessage::System { .. }))
        .take(RECENT_ACTIONS)
        .map(|m| m.to_summary_text(ACTION_SNIPPET_CHARS).replace('\n', " "))
        .collect();
    recent.reverse();
    if !recent.is_empty() {
        lines.push(format!("最近动作（时间正序）：{}", recent.join(" | ")));
    }
    lines
}

/// 扫 System 正文里的 `【…】` 区块标题：去重、限长、限量、保序
fn collect_section_titles(system_text: &str) -> Vec<String> {
    let mut titles: Vec<String> = Vec::new();
    let mut rest = system_text;
    while let Some(start) = rest.find('【') {
        let after = &rest[start + '【'.len_utf8()..];
        let Some(end) = after.find('】') else { break };
        let title = after[..end].trim();
        if !title.is_empty()
            && title.chars().count() <= 32
            && !titles.iter().any(|seen| seen == title)
        {
            titles.push(title.to_string());
            if titles.len() >= MAX_SECTION_TITLES {
                break;
            }
        }
        rest = &after[end + '】'.len_utf8()..];
    }
    titles
}

/// 取某个 `【标题】` 到下一个 `【` 之间的正文，按 token 预算截断
///
/// 用 `find` 得到的字节下标天然落在字符边界上，不做 `&[..N]` 的裸切片
/// —— 中文正文上按字节截会 panic。
fn section_excerpt(
    system_text: &str,
    title: &str,
    snippet_tokens: Option<usize>,
) -> Option<String> {
    let marker = format!("【{title}】");
    let start = system_text.find(&marker)? + marker.len();
    let tail = &system_text[start..];
    let end = tail.find('【').unwrap_or(tail.len());
    let body = tail[..end].trim();
    if body.is_empty() {
        return None;
    }
    Some(match snippet_tokens {
        Some(budget) => truncate_to_tokens(body, budget, "…"),
        None => body.to_string(),
    })
}

/// 小脑模型容量（**两层夹逼后的本次预算**＝`min(软目标, 硬顶)`）
///
/// - 软目标未配置 ⇒ `None`（不裁，既有口径不变）；
/// - 硬顶未配置 ⇒ 直接用软目标；
/// - 软目标 > 硬顶（配置写反 / 推荐值虚高）⇒ 按硬顶封顶：越过最大长度的请求会被
///   provider 直接拒，那才是真超限。
///
/// 取值链收敛到 [`ModelProviderConfig::context_capacity_tokens`]（唯一实现），
/// 本函数只负责「从 Brain 上取」。
pub(crate) fn context_capacity(brain: &Brain) -> Option<u64> {
    brain
        .cerebellum
        .as_ref()
        .and_then(|po| po.config().context_capacity_tokens())
}

/// 单条摘要片段的 token 预算 = 输入预算 × `bp`（容量未配 ⇒ `None` ＝ 不截断）
///
/// `.max(1)` 是 `usize` 的最小非零值，属**结构性下限**而不是拍出来的整数：占比在
/// 极小容量下会向下取整成 0，而「0 token 的片段」等于有内容但看不见，与「不裁」
/// 自相矛盾。
pub(crate) fn snippet_budget(brain: &Brain, bp: u32) -> Option<usize> {
    context_capacity(brain).map(|capacity| {
        let input_budget = usize::try_from(capacity).unwrap_or(usize::MAX);
        bp_of(input_budget, bp).max(1)
    })
}

/// 历史对话单条摘要的 token 预算（四个摘要消费者共用同一条 `SNIPPET_BP`）
pub(crate) fn dialogue_snippet_tokens(brain: &Brain) -> Option<usize> {
    snippet_budget(brain, share::SNIPPET_BP)
}

/// 省略标注（N 由调用方按「注册块数 − 保留块数」算好传入，绝不写死）
fn omission_note(omitted: usize) -> String {
    format!("{OMISSION_NOTE_PREFIX}{omitted} 块）")
}

/// 末块正文按 token 预算截尾，但**至少保留 1 个非空白字符**
///
/// 取舍（与 dal 的「不静默改语义」并列）：**预算服从可解析性** —— 预算再小也宁可
/// 超 1~2 token，绝不产出「裸 `## 标题`」。截断复用
/// `compaction::truncate_to_tokens`，不写第二份 token 估算实现。
fn truncate_body_keep_visible(body: &str, budget: usize) -> String {
    let kept = truncate_to_tokens(body, budget, "");
    if !kept.trim().is_empty() {
        return kept;
    }
    body.trim_start()
        .chars()
        .next()
        .map(|ch| ch.to_string())
        .unwrap_or_default()
}

/// 一条历史记忆压成一行摘要
///
/// `Brain.memories` 在唤醒链路里装的是短期记忆；取不到正文的形态直接返回空串
/// （由调用方按「空则跳过」处理，不塞占位符）。
fn dialogue_line(memory: &Memory, snippet_tokens: Option<usize>) -> String {
    if let MemoryPo::ShortTerm(short_term) = &memory.po {
        let body = short_term.summary.trim();
        return match snippet_tokens {
            Some(budget) => truncate_to_tokens(body, budget, ""),
            None => body.to_string(),
        };
    }
    String::new()
}
#[cfg(test)]
#[path = "cerebellum_context_budget_share_tests.rs"]
mod budget_share_tests;
#[cfg(test)]
#[path = "cerebellum_context_tests.rs"]
mod tests;
