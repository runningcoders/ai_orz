//! Cortex 类型定义
//!
//! 本模块定义 cortex dao 与 brain dal 之间的数据契约：
//! - `ThinkResult`: 模型思考结果（最终回答或工具调用请求）
//! - `ToolDescriptor`: 工具描述符（从业务 Tool 直接派生）
//! - `ToolCallRequest`: 模型发起的工具调用请求
//!
//! 这些类型是「思考层」与「执行层」的边界：
//! - cortex dao 负责「想」→ 返回 ThinkResult
//! - 上层（awakening）负责「做」→ 执行 ToolCallRequest
//! - brain dal 透传 ThinkResult

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 模型思考结果
///
/// 模型返回有两种可能：
/// - `Final`: 最终回答（推理结束，可直接返回给用户）
/// - `ToolCall`: 需要调用工具（由上层执行后将结果拼回 prompt 继续推理）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThinkResult {
    /// 最终回答（纯文本，无 tool_calls）
    Final {
        /// 模型回答内容
        content: String,
        /// 本次调用的 token 使用统计
        usage: TokenUsage,
    },
    /// 工具调用请求（模型要求调用工具）
    ToolCall {
        /// 模型给出的思考内容（可能为空，用于展示推理过程）
        content: Option<String>,
        /// 需要调用的工具列表
        tool_calls: Vec<ToolCallRequest>,
        /// 本次调用的 token 使用统计
        usage: TokenUsage,
    },
}

impl ThinkResult {
    /// 是否为最终回答
    pub fn is_final(&self) -> bool {
        matches!(self, ThinkResult::Final { .. })
    }

    /// 是否为工具调用请求
    pub fn is_tool_call(&self) -> bool {
        matches!(self, ThinkResult::ToolCall { .. })
    }

    /// 获取 token 使用统计
    pub fn usage(&self) -> &TokenUsage {
        match self {
            ThinkResult::Final { usage, .. } => usage,
            ThinkResult::ToolCall { usage, .. } => usage,
        }
    }
}

/// Token 使用统计
///
/// 从 HTTP response body 中直接提取，无需 rig hook。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// 输入 token 数
    pub input_tokens: u64,
    /// 输出 token 数
    pub output_tokens: u64,
    /// 总 token 数（某些 provider 直接返回，否则 input + output）
    pub total_tokens: Option<u64>,
}

impl TokenUsage {
    /// 计算总 token 数
    pub fn total(&self) -> u64 {
        self.total_tokens
            .unwrap_or(self.input_tokens + self.output_tokens)
    }
}

/// 工具描述符
///
/// cortex dao 调用模型时传入的工具定义，遵循 OpenAI function calling 协议。
/// 从业务 `Tool` 结构体直接派生（通过 `From<&Tool>`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// 工具名称（唯一）
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 参数 JSON Schema
    pub parameters: Value,
}

/// 从业务 Tool 直接派生 ToolDescriptor
///
/// cortex dao 只需要工具的描述信息（name/description/parameters），
/// 不需要 CoreTool 的执行逻辑（执行由上层 awakening 负责）。
impl From<&crate::models::tool::Tool> for ToolDescriptor {
    fn from(tool: &crate::models::tool::Tool) -> Self {
        ToolDescriptor {
            name: tool.po.name.clone(),
            description: tool.po.description.clone(),
            parameters: tool
                .po
                .parameters_schema
                .clone()
                .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
        }
    }
}

/// 模型发起的工具调用请求
///
/// 对应 OpenAI Chat Completions response 中的 `tool_calls[i]`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    /// 工具调用 ID（模型生成，回传 tool 结果时需要匹配）
    pub id: String,
    /// 工具名称（对应 ToolDescriptor.name）
    pub name: String,
    /// 参数 JSON（模型生成的参数，已通过 schema 验证）
    pub arguments: Value,
}
