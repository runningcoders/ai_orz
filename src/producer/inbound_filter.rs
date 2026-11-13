//! 渠道入站统一过滤（渠道无关）
//!
//! 各渠道 DAL 完成协议转换后，由 producer 在消息进入内部消息系统前统一判定
//! 「是否值得处理」。本模块只依赖 [`crate::pkg::adapter::AdaptedMessage`] 里
//! 的渠道无关信号（是否群聊 / 是否指向 Agent / 正文），不感知飞书、微信等平台细节。
//!
//! 这样新增渠道时只需在适配层翻译平台语义，过滤规则与判定链路无需重复实现。

use common::enums::MessageRole;

use crate::service::domain::runtime::cerebellum_context::share as context_share;
use crate::service::domain::runtime::compaction::truncate_to_tokens;
use crate::service::domain::runtime::text_budget::tokens_of;

/// 群消息相关性的小脑概率阈值
pub const RELEVANCE_THRESHOLD: f32 = 0.5;

/// 规则层判定结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundGateDecision {
    /// 直接放行
    Process,
    /// 直接丢弃
    Skip,
    /// 放行前先经小脑判断相关性
    Judge,
}

/// 按渠道配置计算入站触发门（纯规则，不含小脑调用）
///
/// - 私聊恒放行
/// - `mention`：仅处理明确指向 Agent 的消息
/// - `prefix`：正文去前导空白后以配置前缀开头才放行；前缀为空时降级为 `mention`
/// - `all`：规则层放行，交由小脑判断
/// - 未知策略按 `mention` 处理
pub fn group_trigger_gate(
    is_group: bool,
    addressed_to_agent: bool,
    trigger: &str,
    prefix: &str,
    content: &str,
) -> InboundGateDecision {
    if !is_group {
        return InboundGateDecision::Process;
    }
    match trigger {
        "all" => InboundGateDecision::Judge,
        "prefix" => {
            let prefix = prefix.trim();
            if prefix.is_empty() {
                return mention_decision(addressed_to_agent);
            }
            if content.trim_start().starts_with(prefix) {
                InboundGateDecision::Process
            } else {
                InboundGateDecision::Skip
            }
        }
        _ => mention_decision(addressed_to_agent),
    }
}

fn mention_decision(addressed_to_agent: bool) -> InboundGateDecision {
    if addressed_to_agent {
        InboundGateDecision::Process
    } else {
        InboundGateDecision::Skip
    }
}

/// 小脑概率是否达到放行阈值；无结论（降级）时放行，避免静默吞消息
pub fn passes_relevance(probability: Option<f32>) -> bool {
    probability.is_none_or(|probability| probability >= RELEVANCE_THRESHOLD)
}

/// 拼接小脑判定上下文（纯函数，可单测）
///
/// 段落顺序固定为「最近群聊 → 相关记忆 → 最近记忆 → 本次消息」；
/// 空段落直接省略，避免把空标题喂给模型。
///
/// 预算**跟随模型最佳上下文**：`capacity_tokens` 由默认小脑 provider 的
/// `min(推荐上下文, 最大上下文)` 提供，各段按 `cerebellum_context::share` 的
/// 万分比分配；容量未配置（`None`）⇒ 不裁，不拍固定字符数。
pub fn build_group_context(
    recent_memories: &[String],
    searched_memories: &[String],
    history: &[(MessageRole, String)],
    current_content: &str,
    sender_id: &str,
    capacity_tokens: Option<u64>,
) -> String {
    let mut sections: Vec<String> = Vec::new();
    // 单条摘要片段预算（历史 / 记忆各占一行，共用同一份额）
    let snippet_budget = tokens_of(capacity_tokens, context_share::SNIPPET_BP);

    let history_lines: Vec<String> = history
        .iter()
        .filter_map(|(role, content)| {
            let text = truncate_to_budget(content.trim(), snippet_budget);
            (!text.is_empty()).then(|| format!("{}: {}", role_label(*role), text))
        })
        .collect();
    push_bounded_section(
        &mut sections,
        "【最近群聊】",
        history_lines,
        tokens_of(capacity_tokens, context_share::INBOUND_HISTORY_BP),
    );
    push_memory_section(
        &mut sections,
        "【相关记忆】",
        searched_memories,
        tokens_of(capacity_tokens, context_share::INBOUND_SEARCHED_MEMORY_BP),
        snippet_budget,
    );
    push_memory_section(
        &mut sections,
        "【最近记忆】",
        recent_memories,
        tokens_of(capacity_tokens, context_share::INBOUND_RECENT_MEMORY_BP),
        snippet_budget,
    );

    let current = format!(
        "发送者: {}\n内容: {}",
        sender_id,
        truncate_to_budget(current_content.trim(), snippet_budget)
    );
    push_bounded_section(
        &mut sections,
        "【本次消息】",
        vec![current],
        tokens_of(capacity_tokens, context_share::INBOUND_CURRENT_BP),
    );
    sections.join("\n\n")
}

/// 按 token 预算追加一个段落（空正文不产生段落）
fn push_bounded_section(
    sections: &mut Vec<String>,
    title: &str,
    lines: Vec<String>,
    budget: Option<usize>,
) {
    let body = truncate_to_budget(&lines.join("\n"), budget);
    if !body.trim().is_empty() {
        sections.push(format!("{title}\n{body}"));
    }
}

fn push_memory_section(
    sections: &mut Vec<String>,
    title: &str,
    memories: &[String],
    budget: Option<usize>,
    snippet_budget: Option<usize>,
) {
    let lines: Vec<String> = memories
        .iter()
        .map(|memory| memory.trim())
        .filter(|memory| !memory.is_empty())
        .map(|memory| format!("- {}", truncate_to_budget(memory, snippet_budget)))
        .collect();
    push_bounded_section(sections, title, lines, budget);
}

/// 按 token 预算截断；`None` ⇒ 不裁（模型容量未配置时的既有口径）
fn truncate_to_budget(text: &str, budget: Option<usize>) -> String {
    match budget {
        Some(budget) => truncate_to_tokens(text, budget, "…"),
        None => text.to_string(),
    }
}

fn role_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "用户",
        MessageRole::Agent => "助手",
        MessageRole::System => "系统",
    }
}

#[cfg(test)]
#[path = "inbound_filter_tests.rs"]
mod tests;
