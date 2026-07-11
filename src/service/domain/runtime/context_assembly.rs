//! Prompt 上下文组装器
//!
//! 【定位】纯函数模块，负责把各种来源的信息组装成模型能理解的格式。
//!
//! 使用 Builder 模式，支持按需扩展不同的 prompt 部分。

use crate::models::agent::Agent;
use crate::models::memory::{Memory, MemoryPo};
use crate::models::message::Message;
use crate::models::user::UserPo;
use common::enums::{ControlMode, ToolStatus};

/// Prompt 构建器
///
/// 链式调用，按需组装不同部分：
/// ```rust, ignore
/// let prompt = PromptBuilder::new()
///     .agent_system(&agent)
///     .history(&memories)
///     .current_message(&message)
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
pub struct PromptBuilder {
    /// 本次思考的 Trace ID（模型输出时可引用此 ID）
    current_trace_id: Option<String>,
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
    /// 工具失败统计：(工具名称, 失败次数)
    tool_failures: Vec<(String, u64)>,
}

impl PromptBuilder {
    /// 创建空的 Builder
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置本次思考的 Trace ID
    ///
    /// 模型可以在输出中引用此 ID，用于追溯完整思考闭环
    pub fn current_trace_id(mut self, trace_id: &str) -> Self {
        self.current_trace_id = Some(trace_id.to_string());
        self
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
        let label = match message.po.message_type {
            common::enums::MessageType::ToolCallResult => "【工具执行结果】",
            common::enums::MessageType::ToolCallRequest => "【工具调用请求】",
            common::enums::MessageType::ConfirmRequest => "【确认请求】",
            common::enums::MessageType::ConfirmResponse => "【确认回复】",
            common::enums::MessageType::TaskAssignment => "【任务分配通知】",
            _ => "【当前消息】",
        };
        self.current_message = Some(format!("{}\n{}", label, message.to_prompt()));
        self
    }

    /// 便捷方法：直接传入消息内容
    ///
    /// 用于测试或简单场景，不需要完整 Message 结构
    pub fn current_message_content(mut self, content: &str) -> Self {
        self.current_message = Some(format!("【当前消息】\n【消息内容】\n{}", content));
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

    /// 添加工具失败统计，用于提示 Agent 谨慎调用
    ///
    /// 当某个工具失败次数较多时，会在 Prompt 中添加警告，
    /// 提醒 Agent 谨慎使用该工具或考虑替代方案。
    pub fn tool_failures(mut self, failures: &[(String, u64)]) -> Self {
        self.tool_failures.extend_from_slice(failures);
        self
    }

    /// 添加 Agent 当前绑定的工具说明。
    ///
    /// 工具自身负责格式化 Prompt 内容；Builder 只做组合。
    /// 注意：`ToolPo::to_tool_prompt()` 不输出协议 config，避免 MCP server
    /// command/env/url/headers 等敏感配置进入模型上下文。
    pub fn agent_tools(mut self, agent: &Agent) -> Self {
        self.tools.extend(
            agent
                .tools()
                .iter()
                .filter(|tool| {
                    matches!(tool.po.control_mode, ControlMode::Manual)
                        && matches!(tool.po.status, ToolStatus::Enabled)
                })
                .map(|tool| tool.po.to_tool_prompt()),
        );
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

        // 0. 本次思考的 Trace ID（放在最最前面，模型能看到并可引用）
        if let Some(trace_id) = &self.current_trace_id {
            result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }

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

        // 6. Manual 工具说明
        if !self.tools.is_empty() {
            result.push_str("【可用 Manual 工具】\n");
            result.push_str(
                "以下仅列出需要通过 ai_orz 消息机制调用的 Manual 工具：如需调用，请发送一条工具调用消息。已经注册到 Rig 的 Auto 工具不在此处列出，仍使用模型默认的 Rig/function calling 调用方式。\n",
            );
            for t in &self.tools {
                result.push_str(&format!("- {}\n", t));
            }
            result.push_str("\n");
        }

        // 6.5 工具失败警告（有失败工具时才显示）
        if !self.tool_failures.is_empty() {
            result.push_str("【工具失败警告】\n");
            result.push_str("以下工具近期失败次数较多，请谨慎使用或考虑替代方案：\n");
            for (tool_name, fail_count) in &self.tool_failures {
                result.push_str(&format!("- {}：失败 {} 次\n", tool_name, fail_count));
            }
            result.push_str("\n");
        }

        // 7. 当前用户消息
        if let Some(msg) = &self.current_message {
            result.push_str(msg);
            result.push_str("\n\n请回复：");
        }

        result
    }
}

/// 便捷函数：快速构建 Agent 对话 Prompt
///
/// 封装了最常用的组合：Trace ID 列表 + Agent 人设 + Agent 绑定工具 + 历史记忆 + 当前消息
pub fn build_conversation_prompt(
    trace_ids: &[String],
    agent: &Agent,
    recent_memories: &[Memory],
    current_message: &Message,
) -> String {
    PromptBuilder::new()
        .trace_ids(trace_ids)
        .agent_system(agent)
        .agent_tools(agent)
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

    #[test]
    fn builder_includes_bound_mcp_tools_without_server_config_details() {
        use crate::models::agent::AgentPo;
        use crate::models::tool::{Tool, ToolPo};
        use common::enums::{ControlMode, ToolProtocol, ToolStatus};
        use serde_json::json;

        let agent_po = AgentPo::new(
            "工具助手".to_string(),
            vec!["助手".to_string()],
            "可以使用工具".to_string(),
            vec!["工具调用".to_string()],
            "按需使用工具。".to_string(),
            "provider-001".to_string(),
            "tester".to_string(),
        );
        let mcp_tool = Tool::from_po_for_management(ToolPo::new(
            "mcp.echo-server.echo".to_string(),
            "mcp.echo-server.echo".to_string(),
            "Echo input text".to_string(),
            ToolProtocol::Mcp,
            json!({
                "server_id": "echo-server",
                "tool_name": "echo",
                "command": "python3 /tmp/private_echo_server.py",
                "env": {"PRIVATE_VALUE": "placeholder-value"},
                "url": "https://internal.example.test/mcp"
            }),
            Some(json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"]
            })),
            vec!["mcp".to_string(), "echo-server".to_string()],
            Some("creator".to_string()),
        ));
        let mut auto_tool_po = ToolPo::new(
            "builtin.auto-tool".to_string(),
            "builtin.auto-tool".to_string(),
            "Auto tool should use Rig default calling".to_string(),
            ToolProtocol::Builtin,
            json!(null),
            Some(json!({"type": "object"})),
            vec!["builtin".to_string()],
            Some("creator".to_string()),
        );
        auto_tool_po.control_mode = ControlMode::Auto;
        let auto_tool = Tool::from_po_for_management(auto_tool_po);
        let mut stale_tool_po = ToolPo::new(
            "mcp.echo-server.stale".to_string(),
            "mcp.echo-server.stale".to_string(),
            "Stale MCP tool should not be visible".to_string(),
            ToolProtocol::Mcp,
            json!({"server_id": "echo-server", "tool_name": "stale"}),
            Some(json!({"type": "object"})),
            vec!["mcp".to_string(), "echo-server".to_string()],
            Some("creator".to_string()),
        );
        stale_tool_po.status = ToolStatus::Stale;
        let stale_tool = Tool::from_po_for_management(stale_tool_po);
        let agent = Agent::from_po_with_tools(agent_po, vec![mcp_tool, auto_tool, stale_tool]);

        let prompt = PromptBuilder::new()
            .agent_system(&agent)
            .agent_tools(&agent)
            .build();

        assert!(prompt.contains("【可用 Manual 工具】"));
        assert!(prompt.contains("Manual 工具"));
        assert!(prompt.contains("工具调用消息"));
        assert!(prompt.contains("消息机制调用"));
        assert!(prompt.contains("Auto 工具"));
        assert!(prompt.contains("模型默认的 Rig/function calling"));
        assert!(prompt.contains("mcp.echo-server.echo"));
        assert!(!prompt.contains("mcp.echo-server.stale"));
        assert!(!prompt.contains("Stale MCP tool should not be visible"));
        assert!(!prompt.contains("builtin.auto-tool"));
        assert!(!prompt.contains("Auto tool should use Rig default calling"));
        assert!(prompt.contains("Echo input text"));
        assert!(prompt.contains("Manual"));
        assert!(prompt.contains("text"));
        assert!(!prompt.contains("python3"));
        assert!(!prompt.contains("PRIVATE_VALUE"));
        assert!(!prompt.contains("placeholder-value"));
        assert!(!prompt.contains("internal.example.test"));
        assert!(!prompt.contains("server_id"));
        assert!(!prompt.contains("tool_name"));
    }
}
