//! Prompt Builder 模块
//!
//! 拆分自原 agent.rs（本次文件重构）：承载 Agent 的 Prompt 构建器实现。
//!
//! - [`default::DefaultPromptBuilder`]：Local Agent 默认实现（角色分离 + 完整区块）
//! - [`flat::FlatPromptBuilder`]：外部 Cli/Remote Agent 扁平化实现
//!
//! 所有区块拼装能力（技能分块 / 通用上下文 / 意图理解 / 回复规则 / 角色拆分等）
//! 均定义在 [`crate::models::prompt_builder::PromptBuilder`] trait 中，
//! `DefaultPromptBuilder` 完整实现，其他 Builder 用不上的方法走 trait 默认空实现。

mod default;
mod flat;

pub use default::DefaultPromptBuilder;
pub use flat::FlatPromptBuilder;

#[cfg(test)]
mod prompt_builder_test;

use crate::models::agent::Agent;
use crate::models::memory::Memory;
use crate::models::message::Message;

/// 便捷函数：快速构建 Agent 对话 Prompt
///
/// 封装了最常用的组合：Trace ID + Agent 人设 + 历史记忆 + 当前消息
pub fn build_conversation_prompt(
    trace_id: &str,
    agent: &Agent,
    recent_memories: &[Memory],
    current_message: &Message,
) -> String {
    use crate::models::prompt_builder::PromptBuilder;
    let mut builder = DefaultPromptBuilder::new();
    builder.current_trace_id(trace_id);
    builder.system_prompt(agent);
    builder.history(recent_memories);
    builder.current_message(current_message);
    builder.build()
}
