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
//! # 容量自截（已裁定：组织器自截，`dal/brain.rs` 一行不动）
//! `dal/brain.rs::shrink_state_to_capacity` 只认 `last_user_message` 键，state 改成
//! `context` 单键后那条兜底不再命中 —— 这属**已发生的等价性缺口**，不是可选优化。
//! 处置已裁定为「组织器自截」：[`ContextBundle::render_within`] 按模型容量从尾部
//! 整块砍（标注省略块数 N），必要时对末块正文截尾，**保证不出现无 body 的裸
//! `## 标题`**。不改 dal 侧的两条理由：块边界（`## 标题` + 正文）是本模块的输出格式，
//! dal 侧要知道它就得硬编码 domain 约定，而 `dal` → `domain` 是反向依赖；且 dal 那个
//! 兜底点是 13 个 scene 共用的，改它要评估全部场景、回滚面也更大。

use crate::models::brain::Brain;
use crate::models::memory::{Memory, MemoryPo};

use super::compaction::{estimate_tokens, truncate_to_tokens};

/// state 里承载分块正文的键名
///
/// 单出口：所有接入场景都经 [`STATE_KEY`] 落到 state，不得各写字符串字面量。
pub(crate) const STATE_KEY: &str = "context";

/// 历史对话块默认条数（尾部取最近 N 条）
pub(crate) const RECENT_DIALOGUE_LIMIT: usize = 3;

/// 单条历史摘要的字符上限（长期/短期记忆都按此截断）
const DIALOGUE_SNIPPET_CHARS: usize = 160;

/// 帧与题面预留（保守值：覆盖 `DecisionRequest` 帧 + 三题型题面）
///
/// 与 `dal/brain.rs::FRAME_RESERVE_TOKENS(512)` 同量级但更保守：本模块只服务已接入的
/// 6 个场景，题面都很小（2 / 4 / 11 个短档），留一倍余量足够。
pub(crate) const CONTEXT_FRAME_RESERVE_TOKENS: usize = 1024;

/// 预算下限：低于它就不做截断（与 `dal::brain::MIN_STATE_BUDGET_TOKENS` 同口径）
pub(crate) const MIN_CONTEXT_BUDGET_TOKENS: usize = 64;

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
    pub(crate) fn with_recent_dialogue(mut self, memories: &[Memory], limit: usize) -> Self {
        let start = memories.len().saturating_sub(limit);
        let mut lines = Vec::new();
        for memory in &memories[start..] {
            let line = dialogue_line(memory);
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
        let Some(budget) = usize::try_from(capacity)
            .unwrap_or(usize::MAX)
            .checked_sub(CONTEXT_FRAME_RESERVE_TOKENS)
        else {
            return self.render();
        };
        if budget < MIN_CONTEXT_BUDGET_TOKENS {
            return self.render();
        }
        self.render_within_tokens(budget)
    }

    /// 真正的按预算渲染（**私有**）
    ///
    /// 私有是为了让测试能构造极小预算走到「末块截尾」分支：经 [`Self::render_within`]
    /// 入口时预算最小是 `MIN_CONTEXT_BUDGET_TOKENS`，到不了 0 边界。
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

/// scene 侧唯一装配点：身份 + 场景 + 用户消息 + 最近对话，并按模型容量自截
///
/// 六个接入场景（三 gate / entry_gate / entry_pool / thinking_strength）都走这里，
/// 不再各写一遍 `Metrics::new().with(...)` 拼装。
pub(crate) fn render_for(brain: &Brain, scene: &str, user_text: &str) -> String {
    ContextBundle::new()
        .with_identity(&brain.agent_id, &brain.agent_name)
        .with_scene(scene)
        .with_user_message(user_text)
        .with_recent_dialogue(&brain.memories, RECENT_DIALOGUE_LIMIT)
        .render_within(context_capacity(brain))
}

/// 小脑模型容量（与 `dal/brain.rs` 的兜底同源同口径：配置 → provider → None）
pub(crate) fn context_capacity(brain: &Brain) -> Option<u64> {
    brain
        .cerebellum
        .as_ref()
        .and_then(|po| po.config().recommended_context_length_tokens())
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
fn dialogue_line(memory: &Memory) -> String {
    if let MemoryPo::ShortTerm(short_term) = &memory.po {
        return truncate_chars(short_term.summary.trim(), DIALOGUE_SNIPPET_CHARS);
    }
    String::new()
}

/// 按字符（非字节）截断，避免切断多字节字符
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 原三键信息搬进正文后必须逐字还在（形态变更最容易的失血点）
    #[test]
    fn render_keeps_identity_values_verbatim() {
        let bundle = ContextBundle::new().with_identity("agent-7f3a", "技术负责人");
        let rendered = bundle.render();
        assert!(
            rendered.contains("## 会话身份"),
            "身份块标题必须出现：{rendered}"
        );
        assert!(
            rendered.contains("agent_id: agent-7f3a"),
            "agent_id 实际取值必须逐字出现在正文：{rendered}"
        );
        assert!(
            rendered.contains("agent_name: 技术负责人"),
            "agent_name 实际取值必须逐字出现在正文：{rendered}"
        );
    }

    /// 取不到上下文就整块跳过，不塞「标题 + 空 body」
    #[test]
    fn empty_dialogue_block_is_skipped_entirely() {
        let bundle = ContextBundle::new()
            .with_scene("wake_gate")
            .with_recent_dialogue(&[], RECENT_DIALOGUE_LIMIT);
        let rendered = bundle.render();
        assert!(
            rendered.contains("## 当前场景"),
            "有内容的块必须出现：{rendered}"
        );
        assert!(
            !rendered.contains("## 最近对话"),
            "空块标题不得出现（不是标题 + 空 body）：{rendered}"
        );
        assert!(!bundle.render().is_empty());
    }

    /// 注册序即渲染序，两次调用逐字节相等
    #[test]
    fn render_is_deterministic_and_follows_registration_order() {
        let build = || {
            ContextBundle::new()
                .with_identity("a-1", "n-1")
                .with_scene("intent_gate")
                .with_user_message("你好")
        };
        let first = build().render();
        let second = build().render();
        assert_eq!(first, second, "同一组输入必须逐字节同输出");
        let identity_at = first.find("## 会话身份").expect("身份块");
        let scene_at = first.find("## 当前场景").expect("场景块");
        let message_at = first.find("## 当前用户消息").expect("消息块");
        assert!(
            identity_at < scene_at && scene_at < message_at,
            "渲染序必须等于注册序：{first}"
        );
    }

    /// 用户消息原文进正文（哨兵串逐字保留，供 gate 层反事实断言复用同一口径）
    #[test]
    fn user_message_goes_into_body_verbatim() {
        let bundle = ContextBundle::new().with_user_message("__SENTINEL_MSG__");
        assert!(bundle.render().contains("__SENTINEL_MSG__"));
    }

    /// 五块夹具：身份 / 场景 / 用户消息 / 附加块一 / 附加块二（注册序固定）
    fn five_block_bundle() -> ContextBundle {
        let mut bundle = ContextBundle::new()
            .with_identity("agent-5b", "五块夹具")
            .with_scene("wake_gate")
            .with_user_message("__SENTINEL_FIVE__");
        bundle.push("附加块一", "alpha-block-body-one".to_string());
        bundle.push(
            "附加块二",
            "第五块正文用于让整块砍可判定，长度必须大于省略标注本身".to_string(),
        );
        bundle
    }

    /// 按 `\n\n` 切块后以 `## ` 开头的段落数（标注紧贴末块正文，不单独成段）
    fn count_blocks(rendered: &str) -> usize {
        rendered
            .split("\n\n")
            .filter(|part| part.starts_with("## "))
            .count()
    }

    /// 每个 `## ` 块的正文都必须非空（不得出现无 body 的裸标题）
    fn assert_every_block_has_body(rendered: &str) {
        for part in rendered.split("\n\n").filter(|p| p.starts_with("## ")) {
            let body = part.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
            assert!(!body.trim().is_empty(), "不得出现无 body 的裸标题：{part}");
        }
    }

    /// 容量为 `None`（未配小脑 provider / 未配上下限）⇒ 逐字节等于 `render()`
    #[test]
    fn render_within_none_matches_render_byte_for_byte() {
        let bundle = five_block_bundle();
        assert_eq!(bundle.render_within(None), bundle.render());
    }

    /// 容量充裕（远超全文）⇒ 同样逐字节等于 `render()`，且不带省略标注
    #[test]
    fn render_within_ample_capacity_matches_render_byte_for_byte() {
        let bundle = five_block_bundle();
        let ample = (estimate_tokens(&bundle.render()) + CONTEXT_FRAME_RESERVE_TOKENS + 512) as u64;
        let rendered = bundle.render_within(Some(ample));
        assert_eq!(rendered, bundle.render());
        assert!(
            !rendered.contains(OMISSION_NOTE_PREFIX),
            "未截断时不得出现省略标注：{rendered}"
        );
    }

    /// 预算紧张时仍不得出现裸 `## 标题`（唯一结构判据）
    #[test]
    fn render_within_tight_budget_never_leaves_bare_title() {
        let bundle = five_block_bundle();
        // 恰好卡在「保留 2 块」的预算（更紧的预算会落到末块截尾分支）
        let keep_two = format!(
            "{}\n\n{}\n{}",
            "## 会话身份\nagent_id: agent-5b\nagent_name: 五块夹具",
            "## 当前场景\nwake_gate",
            "…（上下文已按模型容量截断，省略 3 块）"
        );
        let rendered = bundle.render_within_tokens(estimate_tokens(&keep_two));
        assert_every_block_has_body(&rendered);
        assert_eq!(count_blocks(&rendered), 2);
    }

    /// N 只认并列值（注册 5 / 保留 2 ⇒ N=3；反向 保留 4 ⇒ N=1），不接受只看下界
    #[test]
    fn omission_note_count_is_measured_not_hardcoded() {
        let bundle = five_block_bundle();
        let block0 = "## 会话身份\nagent_id: agent-5b\nagent_name: 五块夹具";
        let block1 = "## 当前场景\nwake_gate";
        let block2 = "## 当前用户消息\n__SENTINEL_FIVE__";
        let block3 = "## 附加块一\nalpha-block-body-one";

        let keep_two = format!("{block0}\n\n{block1}\n…（上下文已按模型容量截断，省略 3 块）");
        let rendered_two = bundle.render_within_tokens(estimate_tokens(&keep_two));
        assert_eq!(rendered_two, keep_two);
        assert_eq!(count_blocks(&rendered_two), 2, "注册 5 块、保留 2 块");

        let keep_four = format!(
            "{block0}\n\n{block1}\n\n{block2}\n\n{block3}\n…（上下文已按模型容量截断，省略 1 块）"
        );
        let rendered_four = bundle.render_within_tokens(estimate_tokens(&keep_four));
        assert_eq!(rendered_four, keep_four);
        assert_eq!(count_blocks(&rendered_four), 4, "注册 5 块、保留 4 块");
    }

    /// 预算 0 ⇒ 走末块截尾分支，标题后至少 1 个非空白字符（先自证该分支可达）
    #[test]
    fn zero_budget_keeps_one_visible_char_after_title() {
        let bundle = five_block_bundle();
        // R50 延伸：先自证「朴素截尾在 0 预算下确实为空」，否则本条走不到边界分支
        let naive = truncate_to_tokens(&bundle.sections[0].body, 0, "");
        assert!(
            naive.trim().is_empty(),
            "0 预算下朴素截尾必须为空，否则截尾分支不可达、本条判据无效"
        );

        let rendered = bundle.render_within_tokens(0);
        assert!(
            rendered.starts_with("## 会话身份\n"),
            "必须保标题：{rendered}"
        );
        let first = rendered
            .strip_prefix("## 会话身份\n")
            .and_then(|rest| rest.chars().next())
            .expect("标题后必须还有正文字符");
        assert!(
            !first.is_whitespace(),
            "截尾后 body 至少保留 1 个非空白字符：{rendered:?}"
        );
        assert_every_block_has_body(&rendered);
        assert!(
            rendered.contains(TAIL_TRUNCATION_NOTE) || rendered.contains(OMISSION_NOTE_PREFIX),
            "截尾必须留标注：{rendered}"
        );
    }

    /// 全部块都空 ⇒ 渲染为空串（调用方可据此判「没东西可带」）
    #[test]
    fn all_empty_blocks_render_empty() {
        let bundle = ContextBundle::new()
            .with_identity("", "")
            .with_scene("")
            .with_user_message("   ");
        assert_eq!(bundle.render(), "", "全空块必须渲染成空串");
    }

    /// 缺口 1（前台 01a1c645 裁定）：**截尾路径**也要跑一遍完整结构判据。
    ///
    /// 砍块路径（`render_kept`）与截尾路径（保标题 + body 截尾）的拼装不一样，
    /// #3 只覆盖了前者、#5 只断了首字符非空白 ⇒ 这里补「每段首行 `## ` 且非空行数 ≥ 2」。
    #[test]
    fn single_block_tail_truncation_passes_full_structure_criteria() {
        let bundle = ContextBundle::new().with_identity("agent-tail", "截尾夹具");
        assert_eq!(bundle.sections.len(), 1, "夹具必须恰一块才能走到 TAIL 分支");
        let rendered = bundle.render_within_tokens(0);
        for part in rendered.split("\n\n") {
            assert!(!part.trim().is_empty(), "不得出现空段落：{rendered:?}");
            let first = part.lines().next().unwrap_or("");
            assert!(first.starts_with("## "), "每块首行必须是二级标题：{part:?}");
            let non_empty = part.lines().filter(|line| !line.trim().is_empty()).count();
            assert!(non_empty >= 2, "每块至少标题 + 正文两行：{part:?}");
        }
        assert_every_block_has_body(&rendered);
    }

    /// 缺口 1（多块侧）：`total > 1` 的截尾走「省略 N 块」标注，同样要过结构判据。
    #[test]
    fn multi_block_tail_truncation_passes_full_structure_criteria() {
        let mut bundle = ContextBundle::new()
            .with_identity("agent-long", "长块夹具")
            .with_scene("wake_gate");
        // 首块正文撑到远超「第二块 + 标注」的成本 ⇒ 只有截尾分支能落地
        bundle.sections[0].body = "首块正文".repeat(200);
        let rendered = bundle.render_within_tokens(50);
        assert!(
            rendered.contains(&omission_note(1)),
            "被砍 1 块必须写成「省略 1 块」：{rendered:?}"
        );
        for part in rendered.split("\n\n") {
            assert!(!part.trim().is_empty(), "不得出现空段落：{rendered:?}");
            let first = part.lines().next().unwrap_or("");
            assert!(first.starts_with("## "), "每块首行必须是二级标题：{part:?}");
            let non_empty = part.lines().filter(|line| !line.trim().is_empty()).count();
            assert!(non_empty >= 2, "每块至少标题 + 正文两行：{part:?}");
        }
        assert_every_block_has_body(&rendered);
    }

    /// 缺口 2（前台裁定）：`TAIL_TRUNCATION_NOTE` 分支的语义断言 ——
    /// 一块都没被砍，全文必须出现截尾标注、且**不得**残留「省略 N 块」。
    #[test]
    fn tail_truncation_note_never_claims_blocks_were_omitted() {
        let bundle = ContextBundle::new().with_identity("agent-tail", "截尾夹具");
        assert_eq!(bundle.sections.len(), 1, "夹具必须恰一块");
        let rendered = bundle.render_within_tokens(0);
        assert!(
            rendered.contains(TAIL_TRUNCATION_NOTE),
            "单块截尾必须用 TAIL_TRUNCATION_NOTE：{rendered:?}"
        );
        assert!(
            !rendered.contains("省略"),
            "没有任何块被砍，不得误标「省略」：{rendered:?}"
        );
    }
}
