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
//! builder.skills(&skills);
//! builder.history(&memories);
//! builder.current_message(&message);
//! let prompt = builder.build();
//! ```
//!
//! 【分块拼装】skills 统一注入，build() 时按 tag 自动分块：
//! - neural tag 的技能 → 神经技能区块（所有 Agent 必加载）
//! - 其他技能 → 必加载技能区块（按 agent roles ∪ installed_tags 匹配）

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

    /// 设置 Agent 人设 / System Prompt
    ///
    /// 同时缓存 agent 的 roles + installed_tags 作为后续技能分块的匹配键。
    fn system_prompt(&mut self, agent: &Agent);

    /// 设置历史对话记忆
    fn history(&mut self, memories: &[Memory]);

    /// 设置当前用户消息
    fn current_message(&mut self, message: &Message);

    /// 设置 Agent 可用技能（全量注入，build 时按 tag 分块）
    fn skills(&mut self, skills: &[SkillPo]);

    /// 设置工具失败统计
    fn tool_failures(&mut self, failures: &[(String, u64)]);

    /// 设置用户画像
    fn user_profile(&mut self, user: &UserPo);

    /// 设置项目上下文（消息关联的项目实体摘要）
    fn project_context(&mut self, project: &crate::models::project::Project);

    /// 设置任务上下文（消息关联的任务实体摘要）
    fn task_context(&mut self, task: &crate::models::task::Task);

    /// 构建最终的 Prompt 字符串
    ///
    /// 使用 `&self` 而非 `self`，支持重复构建和 trait object 使用。
    fn build(&self) -> String;

    /// 构建沉淀场景的 Prompt（与 build() 对称）
    ///
    /// 复用已挂载的 system_prompt/skills/user_profile/project_context/task_context/history，
    /// 加上沉淀约束章节（不发消息、只用记忆工具）和待沉淀短期记忆摘要，生成最终模板。
    /// 不使用 current_message（沉淀场景无用户消息）。
    ///
    /// `trace_ids` 为本次沉淀所依赖的 trace 列表，写入 prompt 要求 Agent 调用
    /// save_short_term_memory 时填入 trace_ids 字段，保证记忆可追溯。
    ///
    /// 默认实现回退到 build()，仅 DefaultPromptBuilder 真正实现沉淀语义
    /// （Cli/Remote Agent 不参与沉淀，不会走到此分支）。
    fn build_sleep_prompt(&self, pending_memories_summary: &str, trace_ids: &[String]) -> String {
        let _ = (pending_memories_summary, trace_ids);
        self.build()
    }

    /// 构建总结退出场景的 Prompt
    ///
    /// 当思考轮次耗尽时，或正常完成时，用此 prompt 让 Agent 总结当前工作进展、遇到的问题，
    /// 并通过消息工具（send_message 等）或任务工具（update_task_progress 等）
    /// 将总结发送给消息源或记录到 task 中。
    ///
    /// `work_summary` 为当前工作对话的摘要文本（由 messages_to_summary 生成）。
    /// `total_rounds` 为累计消耗的思考轮次。
    /// `trace_ids` 为本次总结所依赖的 trace 列表，写入 prompt 要求 Agent 调用
    /// save_short_term_memory 时填入 trace_ids 字段，保证记忆可追溯。
    ///
    /// 默认实现回退到 build()，仅 DefaultPromptBuilder 真正实现总结语义。
    fn build_summary_prompt(
        &self,
        work_summary: &str,
        total_rounds: usize,
        trace_ids: &[String],
    ) -> String {
        let _ = (work_summary, total_rounds, trace_ids);
        self.build()
    }
}
