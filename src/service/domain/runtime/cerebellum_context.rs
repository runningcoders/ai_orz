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
//! ⚠️ 已知边界（如实登记，处置待裁）：`dal/brain.rs::shrink_state_to_capacity`
//! 只对 `last_user_message` 键做容量对齐；改成 `context` 键后该兜底不再命中。
//! 好处是绝不出现「半个 `## 标题`」（根本不截），代价是超长输入不再被容量对齐。

use crate::models::memory::{Memory, MemoryPo};

/// state 里承载分块正文的键名
///
/// 单出口：所有接入场景都经 [`STATE_KEY`] 落到 state，不得各写字符串字面量。
pub(crate) const STATE_KEY: &str = "context";

/// 历史对话块默认条数（尾部取最近 N 条）
pub(crate) const RECENT_DIALOGUE_LIMIT: usize = 3;

/// 单条历史摘要的字符上限（长期/短期记忆都按此截断）
const DIALOGUE_SNIPPET_CHARS: usize = 160;

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

    /// 全部块都空 ⇒ 渲染为空串（调用方可据此判「没东西可带」）
    #[test]
    fn all_empty_blocks_render_empty() {
        let bundle = ContextBundle::new()
            .with_identity("", "")
            .with_scene("")
            .with_user_message("   ");
        assert_eq!(bundle.render(), "", "全空块必须渲染成空串");
    }
}
