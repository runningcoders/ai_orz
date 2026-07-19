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
//! builder.history(&memories);
//! let prompt = builder.build();
//! ```

use crate::models::agent::Agent;
use crate::models::memory::Memory;
use crate::models::message::Message;
use crate::models::skill::SkillPo;
use crate::models::user::UserPo;

/// Prompt 构建器 trait
///
/// 统一的 Prompt 组装接口，不同 Agent 类型可提供不同实现。
pub trait PromptBuilder: Send + Sync {
    /// 设置本次思考的 Trace ID
    fn current_trace_id(&mut self, trace_id: &str);

    /// 批量设置关联 Trace ID 列表
    fn trace_ids(&mut self, trace_ids: &[String]);

    /// 设置 Agent 人设 / System Prompt
    fn system_prompt(&mut self, agent: &Agent);

    /// 设置历史对话记忆
    fn history(&mut self, memories: &[Memory]);

    /// 设置当前用户消息
    fn current_message(&mut self, message: &Message);

    /// 设置 Agent 可用技能
    fn agent_skills(&mut self, skills: &[SkillPo]);

    /// 设置 Agent 绑定的 Manual 工具
    fn bound_tools(&mut self, agent: &Agent);

    /// 设置内置工具（神经工具 + 已安装工具包）的 prompt 说明
    ///
    /// 与 `bound_tools` 互补：bound_tools 注入 Agent 显式绑定的 Manual 工具，
    /// builtin_tools 注入 Agent 天生拥有或通过工具包安装的内置工具。
    fn builtin_tools(&mut self, tool_prompts: &[String]);

    /// 设置工具失败统计
    fn tool_failures(&mut self, failures: &[(String, u64)]);

    /// 设置用户画像
    fn user_profile(&mut self, user: &UserPo);

    /// 构建最终的 Prompt 字符串
    ///
    /// 使用 `&self` 而非 `self`，支持重复构建和 trait object 使用。
    fn build(&self) -> String;
}
