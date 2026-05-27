//! Prompt 上下文组装器
//!
//! 【定位】纯函数模块，负责把各种来源的信息组装成模型能理解的格式。
//!
//! 使用 Builder 模式，支持按需扩展不同的 prompt 部分。

use crate::models::agent::Agent;
use crate::models::memory::{Memory, MemoryPo};
use crate::models::message::Message;
use crate::models::user::UserPo;

/// Prompt 构建器
///
/// 链式调用，按需组装不同部分：
/// ```rust
/// let prompt = PromptBuilder::new()
///     .agent_system(&agent)
///     .history(&memories)
///     .current_message(&message)
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
pub struct PromptBuilder {
    /// 关联的 Trace ID 列表（多个 trace 的总结）
    trace_ids: Vec<String>,
    /// Agent 人设 / System Prompt
    system_prompt: Option<String>,
    /// 用户画像信息（仅客服类 Agent 使用）
    user_profile: Option<String>,
    /// 历史对话记忆
    history: Vec<String>,
    /// 当前用户消息
    current_message: Option<String>,
    /// （预留）技能说明
    skills: Vec<String>,
    /// （预留）工具说明
    tools: Vec<String>,
}

impl PromptBuilder {
    /// 创建空的 Builder
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加关联 Trace ID
    ///
    /// 用于关联本次对话涉及的多个 trace，Agent 在输出总结时引用这些 ID
    pub fn add_trace_id(mut self, trace_id: &str) -> Self {
        self.trace_ids.push(trace_id.to_string());
        self
    }

    /// 批量添加关联 Trace ID
    pub fn trace_ids(mut self, trace_ids: &[String]) -> Self {
        self.trace_ids.extend_from_slice(trace_ids);
        self
    }

    /// 添加 Agent 人设
    ///
    /// 调用 Agent::to_system_prompt() 生成标准格式的 System Prompt
    /// Agent 相关的所有 prompt 格式化逻辑都内聚在 AgentPo 内部
    pub fn agent_system(mut self, agent: &Agent) -> Self {
        self.system_prompt = Some(agent.to_system_prompt());
        self
    }

    /// 添加历史对话记忆
    ///
    /// 调用 Memory::to_prompt_summary() 提取记忆摘要
    /// 所有记忆格式化逻辑都内聚在 MemoryPo 内部
    pub fn history(mut self, memories: &[Memory]) -> Self {
        for memory in memories {
            if let Some(summary) = memory.to_prompt_summary() {
                self.history.push(summary);
            }
        }
        self
    }

    /// 添加当前用户消息
    ///
    /// 调用 Message::to_prompt() 生成标准格式的消息内容
    /// 所有消息格式化逻辑都内聚在 MessagePo 内部
    pub fn current_message(mut self, message: &Message) -> Self {
        self.current_message = Some(message.to_prompt());
        self
    }

    /// 便捷方法：直接传入消息内容
    ///
    /// 用于测试或简单场景，不需要完整 Message 结构
    pub fn current_message_content(mut self, content: &str) -> Self {
        self.current_message = Some(format!("【消息内容】\n{}", content));
        self
    }

    /// （预留）添加技能说明
    pub fn skills(mut self, skills: &[String]) -> Self {
        self.skills.extend(skills.iter().cloned());
        self
    }

    /// （预留）添加工具说明
    pub fn tools(mut self, tools: &[String]) -> Self {
        self.tools.extend(tools.iter().cloned());
        self
    }

    /// 添加用户画像信息
    ///
    /// 【使用场景】仅客服类 Agent 需要使用，包含：
    /// - 用户基础信息
    /// - 用户喜好/偏好（动态补充）
    /// - 历史服务记录摘要（动态补充）
    pub fn user_profile(mut self, user_profile: &str) -> Self {
        self.user_profile = Some(user_profile.to_string());
        self
    }

    /// 便捷方法：从 UserPo 生成用户基础信息并添加到 Prompt
    ///
    /// 调用 UserPo::to_basic_info_prompt() 生成标准格式
    /// 如果需要额外补充偏好/历史记录，可以继续调用 user_profile() 追加
    pub fn user_basic_info(mut self, user: &UserPo) -> Self {
        self.user_profile = Some(user.to_basic_info_prompt());
        self
    }

    /// 构建最终的 Prompt 字符串
    pub fn build(self) -> String {
        let mut result = String::new();

        // 1. 关联的 Trace ID 列表（放在最前面，方便 Agent 引用）
        if !self.trace_ids.is_empty() {
            result.push_str(&format!(
                "【关联 Trace IDs】{}\n\n",
                self.trace_ids.join(", ")
            ));
        }

        // 2. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 3. 用户画像信息（仅客服类 Agent 有这部分）
        if let Some(profile) = &self.user_profile {
            result.push_str("【用户画像】\n");
            result.push_str(profile);
            result.push_str("\n\n");
        }

        // 4. 历史对话记忆
        if !self.history.is_empty() {
            result.push_str("【历史对话】\n");
            for h in &self.history {
                result.push_str(h);
                result.push_str("\n");
            }
            result.push_str("\n");
        }

        // 5. （预留）技能说明
        if !self.skills.is_empty() {
            result.push_str("【可用技能】\n");
            for s in &self.skills {
                result.push_str(&format!("- {}\n", s));
            }
            result.push_str("\n");
        }

        // 6. （预留）工具说明
        if !self.tools.is_empty() {
            result.push_str("【可用工具】\n");
            for t in &self.tools {
                result.push_str(&format!("- {}\n", t));
            }
            result.push_str("\n");
        }

        // 7. 当前用户消息
        if let Some(msg) = &self.current_message {
            result.push_str("【当前消息】\n");
            result.push_str(msg);
            result.push_str("\n\n请回复：");
        }

        result
    }
}

/// 便捷函数：快速构建 Agent 对话 Prompt
///
/// 封装了最常用的组合：Trace ID 列表 + Agent 人设 + 历史记忆 + 当前消息
pub fn build_conversation_prompt(
    trace_ids: &[String],
    agent: &Agent,
    recent_memories: &[Memory],
    current_message: &Message,
) -> String {
    PromptBuilder::new()
        .trace_ids(trace_ids)
        .agent_system(agent)
        .history(recent_memories)
        .current_message(current_message)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_empty() {
        let prompt = PromptBuilder::new().build();
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_builder_only_system() {
        // 创建一个最小的 Agent 用于测试
        use crate::models::agent::AgentPo;

        let agent_po = AgentPo::new(
            "测试助手".to_string(),
            vec!["助手".to_string()],
            "我是一个测试助手".to_string(),
            vec!["测试能力".to_string()],
            "你是一个严谨、专业、乐于助人的助手。总是给出准确、有用的回答。".to_string(),
            "provider-001".to_string(),
            "tester".to_string(),
        );
        let agent = Agent::from_po(agent_po);

        let prompt = PromptBuilder::new().agent_system(&agent).build();

        assert!(prompt.contains("【Agent ID】"));
        assert!(prompt.contains("【Agent 名称】"));
        assert!(prompt.contains("测试助手"));
        assert!(prompt.contains("【角色描述】"));
        assert!(prompt.contains("【灵魂设定】"));
        assert!(prompt.contains("严谨、专业、乐于助人"));
    }
}
