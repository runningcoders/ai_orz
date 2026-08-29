//! FlatPromptBuilder——外部 Agent（Cli/Remote）扁平化 Prompt 构建器
//!
//! 拆分自原 agent.rs（本次文件重构）：把角色分离的消息合成为单条纯文本提示词。
//!
//! # 为什么需要
//!
//! `DefaultPromptBuilder` 产出 OpenAI Chat 协议的 `[System, User]` 角色分离结构，
//! 用于配合 function calling。但外部 Agent 走的是「一段纯文本」协议：
//! - Cli：prompt 通过 stdin 传给子进程（`dao/agent_runtime/codex.rs`）
//! - Remote：prompt 作为 A2A `message.parts[].text` 发送
//!
//! 二者都不解析 Chat role，而 `BrainDal::think` 对 Cli/Remote 分支只取最后一条
//! `ChatMessage::User`（`extract_last_user_prompt`）。若直接复用 `DefaultPromptBuilder`，
//! System 区块（人设 + 技能 + 回复规则）会被整块丢弃——外部 Agent 等于裸跑。
//!
//! # 设计
//!
//! 内层复用 [`super::default::DefaultPromptBuilder`] 的全部区块拼装逻辑，
//! 仅在「初始消息」阶段把多条消息合成为单条，保证 brain 层按角色提取时不丢内容，
//! 且与 Local Agent 输入对齐。用不上的区块拼装方法走
//! [`crate::models::prompt_builder::PromptBuilder`] trait 默认空实现。
//!
//! 每类 Agent Dal 可重写 `prompt_builder()` 换成自己的 Builder 实现，
//! 以适配具体 CLI 的推荐格式（不同 CLI 对 system/user 的摆放要求不同）。

use crate::models::agent::Agent;
use crate::models::cortex_types::ChatMessage;
use crate::models::memory::Memory;
use crate::models::message::Message;
use crate::models::skill::SkillPo;
use crate::models::user::UserPo;

use super::default::DefaultPromptBuilder;
// ==================== 外部 Agent（Cli/Remote）扁平化 Builder ====================

/// 模板占位符：System 部分（人设 + 技能 + 场景规则）
const PH_SYSTEM: &str = "{system}";
/// 模板占位符：User 部分（上下文 + 历史 + 当前消息）
const PH_USER: &str = "{user}";
/// 模板占位符：完整提示词（System + User，缺省拼接结果）
const PH_PROMPT: &str = "{prompt}";

/// 外部 Agent 提示词构建器：把角色分离的消息合成为单条纯文本提示词
///
/// # 为什么需要
///
/// `DefaultPromptBuilder` 产出 OpenAI Chat 协议的 `[System, User]` 角色分离结构，
/// 用于配合 function calling。但外部 Agent 走的是「一段纯文本」协议：
/// - Cli：prompt 通过 stdin 传给子进程（`dao/agent_runtime/codex.rs`）
/// - Remote：prompt 作为 A2A `message.parts[].text` 发送
///
/// 二者都不解析 Chat role，而 `BrainDal::think` 对 Cli/Remote 分支只取最后一条
/// `ChatMessage::User`（`extract_last_user_prompt`）。若直接复用 `DefaultPromptBuilder`，
/// System 区块（人设 + 技能 + 回复规则）会被整块丢弃——外部 Agent 等于裸跑。
///
/// # 设计
///
/// 内层复用 `DefaultPromptBuilder` 的全部区块拼装逻辑，仅在「初始消息」阶段把多条
/// 消息合成为单条，保证 brain 层按角色提取时不丢内容，且与 Local Agent 输入对齐。
///
/// 每类 Agent Dal 可重写 `prompt_builder()` 换成自己的 Builder 实现，
/// 以适配具体 CLI 的推荐格式（不同 CLI 对 system/user 的摆放要求不同）。
pub struct FlatPromptBuilder {
    inner: DefaultPromptBuilder,
    /// prompt 模板（来自 `ExternalAgentConfig::Cli.prompt_template`）
    pub(crate) prompt_template: Option<String>,
}

impl FlatPromptBuilder {
    /// 创建空的 Builder
    pub fn new() -> Self {
        Self {
            inner: DefaultPromptBuilder::new(),
            prompt_template: None,
        }
    }

    /// 按模板装配提示词
    ///
    /// 支持的占位符（与 `dao/agent_runtime/codex.rs` 的 `{prompt}` 约定兼容）：
    /// - `{system}`：System 部分（人设 + 技能 + 场景规则）
    /// - `{user}`：User 部分（上下文 + 历史 + 当前消息）
    /// - `{prompt}`：完整提示词，即未配置模板时的缺省拼接结果
    ///
    /// 未配置模板时按 `{system}\n\n{user}` 拼接，与 Local Agent 的输入词保持一致。
    fn apply_template(&self, system: &str, user: &str) -> String {
        let full = match (system.is_empty(), user.is_empty()) {
            (true, true) => String::new(),
            (true, false) => user.to_string(),
            (false, true) => system.to_string(),
            (false, false) => format!("{system}\n\n{user}"),
        };
        match &self.prompt_template {
            None => full.clone(),
            Some(template) => template
                .replace(PH_SYSTEM, system)
                .replace(PH_USER, user)
                .replace(PH_PROMPT, &full),
        }
    }

    /// 把角色分离的多条消息合成为单条 User 消息
    ///
    /// Assistant / Tool 消息在初始消息阶段不会出现（它们由 think loop 追加），
    /// 这里一并忽略，避免出现非预期的拼接。
    fn flatten(&self, messages: &[ChatMessage]) -> Vec<ChatMessage> {
        let mut system = String::new();
        let mut user = String::new();
        for m in messages {
            match m {
                ChatMessage::System { content } => {
                    if !system.is_empty() {
                        system.push_str("\n\n");
                    }
                    system.push_str(content);
                }
                ChatMessage::User { content } => {
                    if !user.is_empty() {
                        user.push_str("\n\n");
                    }
                    user.push_str(content);
                }
                _ => {}
            }
        }
        vec![ChatMessage::user(self.apply_template(&system, &user))]
    }

    /// 合成并返回纯文本（供 trace / stat 记录使用）
    fn flatten_text(&self, messages: &[ChatMessage]) -> String {
        self.flatten(messages)
            .into_iter()
            .filter_map(|m| match m {
                ChatMessage::User { content } => Some(content),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl Default for FlatPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::models::prompt_builder::PromptBuilder for FlatPromptBuilder {
    fn current_trace_id(&mut self, trace_id: &str) {
        self.inner.current_trace_id(trace_id);
    }

    /// 挂载人设，并顺带捕获该外部 Agent 的 prompt 模板
    fn system_prompt(&mut self, agent: &Agent) {
        self.prompt_template = agent
            .brain
            .as_ref()
            .and_then(|b| b.runtime_config.external_config.as_ref())
            .and_then(|cfg| match cfg {
                crate::models::agent::ExternalAgentConfig::Cli {
                    prompt_template, ..
                } => prompt_template.clone(),
                // A2A Remote 暂无模板配置，走缺省拼接
                crate::models::agent::ExternalAgentConfig::Remote { .. } => None,
            });
        self.inner.system_prompt(agent);
    }

    fn history(&mut self, memories: &[Memory]) {
        self.inner.history(memories);
    }

    fn settled_reference(&mut self, items: &[String]) {
        self.inner.settled_reference(items);
    }

    fn compacted_context(&mut self, summary: &str) {
        self.inner.compacted_context(summary);
    }

    fn past_memories_reference(&mut self, items: &[String]) {
        self.inner.past_memories_reference(items);
    }

    fn current_message(&mut self, message: &Message) {
        self.inner.current_message(message);
    }

    fn skills(&mut self, skills: &[SkillPo]) {
        self.inner.skills(skills);
    }

    fn tool_failures(&mut self, failures: &[(String, u64)]) {
        self.inner.tool_failures(failures);
    }

    fn user_profile(&mut self, user: &UserPo) {
        self.inner.user_profile(user);
    }

    fn project_context(&mut self, project: &crate::models::project::Project) {
        self.inner.project_context(project);
    }

    fn task_context(&mut self, task: &crate::models::task::Task) {
        self.inner.task_context(task);
    }

    fn workspace_context(
        &mut self,
        default_workspace: String,
        user_home: String,
        user_shared_workspace: String,
        user_agent_workspace: Option<String>,
        agent_workspace: Option<String>,
        project_workspace: Option<String>,
    ) {
        self.inner.workspace_context(
            default_workspace,
            user_home,
            user_shared_workspace,
            user_agent_workspace,
            agent_workspace,
            project_workspace,
        );
    }

    fn build(&self) -> String {
        // 与 build_initial_messages 同源，保证 trace 记录与真实输入一致
        self.apply_template(
            &self.inner.awaken_system_part(),
            &self.inner.awaken_user_part(),
        )
    }

    fn build_sleep_prompt(&self, pending_memories_summary: &str, trace_ids: &[String]) -> String {
        self.flatten_text(
            &self
                .inner
                .build_sleep_initial_messages(pending_memories_summary, trace_ids),
        )
    }

    fn build_summary_prompt(
        &self,
        work_summary: &str,
        total_rounds: usize,
        trace_ids: &[String],
    ) -> String {
        self.flatten_text(&self.inner.build_summary_initial_messages(
            work_summary,
            total_rounds,
            trace_ids,
        ))
    }

    fn build_intent_analyze_prompt(&self) -> String {
        self.flatten_text(&self.inner.build_intent_analyze_initial_messages())
    }

    fn intent_analysis(
        &mut self,
        analysis: &crate::service::domain::runtime::awakening::IntentAnalysis,
    ) {
        self.inner.intent_analysis(analysis);
    }

    fn build_initial_messages(&self) -> Vec<ChatMessage> {
        self.flatten(&self.inner.build_initial_messages())
    }

    fn build_sleep_initial_messages(
        &self,
        pending_memories_summary: &str,
        trace_ids: &[String],
    ) -> Vec<ChatMessage> {
        self.flatten(
            &self
                .inner
                .build_sleep_initial_messages(pending_memories_summary, trace_ids),
        )
    }

    fn build_summary_initial_messages(
        &self,
        work_summary: &str,
        total_rounds: usize,
        trace_ids: &[String],
    ) -> Vec<ChatMessage> {
        self.flatten(&self.inner.build_summary_initial_messages(
            work_summary,
            total_rounds,
            trace_ids,
        ))
    }

    fn build_intent_analyze_initial_messages(&self) -> Vec<ChatMessage> {
        self.flatten(&self.inner.build_intent_analyze_initial_messages())
    }
}
