//! Prompt Builder trait 定义
//!
//! 【定位】纯抽象层，定义 Prompt 组装的统一接口。
//! 不同类型的 Agent（Local、Cli、Remote）可以有各自的实现。
//!
//! 设计：使用 `&mut self` 风格的方法，支持 trait object（`Box<dyn PromptBuilder>`）。
//! 调用方式：
//! ```rust, ignore
//! let mut builder = agent_dal.prompt_builder();
//! builder.current_trace_id(&trace_id);
//! builder.system_prompt(&agent);
//! builder.tools(&tools);
//! builder.skills(&skills);
//! builder.history(&memories);
//! builder.current_message(&message);
//! let prompt = builder.build();
//! ```
//!
//! 【分块拼装】tools 和 skills 统一注入，build() 时按 tag 自动分块：
//! - neural tag 的工具/技能 → 神经工具/神经技能区块（所有 Agent 必加载）
//! - 其他工具/技能 → 常用工具/必加载技能区块（按 agent roles ∪ installed_tags 匹配）

use crate::models::agent::Agent;
use crate::models::memory::Memory;
use crate::models::message::Message;
use crate::models::skill::SkillPo;
use crate::models::tool::ToolPo;
use crate::models::user::UserPo;

/// Prompt 构建器 trait
///
/// 统一的 Prompt 组装接口，不同 Agent 类型可提供不同实现。
pub trait PromptBuilder: Send + Sync {
    /// 设置本次思考的 Trace ID
    fn current_trace_id(&mut self, trace_id: &str);

    /// 设置 Agent 人设 / System Prompt
    ///
    /// 同时缓存 agent 的 roles + installed_tags 作为后续工具/技能分块的匹配键。
    fn system_prompt(&mut self, agent: &Agent);

    /// 设置历史对话记忆
    fn history(&mut self, memories: &[Memory]);

    /// 设置当前用户消息
    fn current_message(&mut self, message: &Message);

    /// 设置 Agent 可用技能（全量注入，build 时按 tag 分块）
    fn skills(&mut self, skills: &[SkillPo]);

    /// 设置 Agent 可用工具（全量注入，build 时按 tag 分块）
    ///
    /// 传入 ToolPo 列表（Tool 实体不可 Clone，使用 PO 足够生成 Prompt）。
    fn tools(&mut self, tools: &[ToolPo]);

    /// 设置工具失败统计
    fn tool_failures(&mut self, failures: &[(String, u64)]);

    /// 设置用户画像
    fn user_profile(&mut self, user: &UserPo);

    /// 构建最终的 Prompt 字符串
    ///
    /// 使用 `&self` 而非 `self`，支持重复构建和 trait object 使用。
    fn build(&self) -> String;
}
