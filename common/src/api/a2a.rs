//! A2A (Agent-to-Agent) Protocol API 类型
//!
//! 定义 A2A 协议 v0.3.0 的核心实体，前后端共享。
//! 参考：https://github.com/google/A2A

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ===== Agent Card =====

/// Agent Card — 对外暴露的组织能力描述
///
/// 通过 `GET /.well-known/agent.json` 公开访问，无需认证。
/// 对外只暴露一个统一入口（组织级），不列具体内部 Agent。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentCard {
    /// 组织名称
    pub name: String,
    /// 组织描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 协议版本（如 "0.3.0"）
    pub version: String,
    /// 协议端点 URL（如 "http://host/a2a"）
    pub url: String,
    /// 能力声明
    pub capabilities: AgentCapabilities,
    /// 对外技能列表（组织级，非具体 Agent）
    #[serde(default)]
    pub skills: Vec<AgentSkill>,
    /// 默认输入模式
    pub default_input_modes: Vec<String>,
    /// 默认输出模式
    pub default_output_modes: Vec<String>,
}

/// Agent 能力声明
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentCapabilities {
    /// 是否支持 SSE 流式
    #[serde(default)]
    pub streaming: bool,
    /// 是否支持推送通知
    #[serde(default)]
    pub push_notifications: bool,
}

/// Agent 技能描述
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentSkill {
    /// 技能 ID
    pub id: String,
    /// 技能名称
    pub name: String,
    /// 技能描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 标签列表
    #[serde(default)]
    pub tags: Vec<String>,
}

// ===== JSON-RPC 2.0 =====

/// JSON-RPC 2.0 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// 协议版本，固定 "2.0"
    pub jsonrpc: String,
    /// 请求 ID（string | number | null）
    pub id: Value,
    /// 方法名
    pub method: String,
    /// 参数
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC 2.0 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// 协议版本
    pub jsonrpc: String,
    /// 请求 ID（与请求对应）
    pub id: Value,
    /// 成功结果（与 error 互斥）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// 错误信息（与 result 互斥）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// 成功响应
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// 错误响应
    pub fn error(id: Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message, data: None }),
        }
    }
}

/// JSON-RPC 错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// 错误码
    pub code: i32,
    /// 错误消息
    pub message: String,
    /// 附加数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// JSON-RPC 标准错误码
pub mod error_codes {
    /// 解析错误
    pub const PARSE_ERROR: i32 = -32700;
    /// 无效请求
    pub const INVALID_REQUEST: i32 = -32600;
    /// 方法未找到
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// 参数无效
    pub const INVALID_PARAMS: i32 = -32602;
    /// 内部错误
    pub const INTERNAL_ERROR: i32 = -32603;
}

// ===== Task =====

/// A2A Task — 对应 ai_orz 的 Project
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct A2aTask {
    /// Task ID（= ai_orz project id）
    pub id: String,
    /// 会话 ID（用于多轮对话关联）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// 任务状态
    pub status: A2aTaskStatus,
    /// 消息流
    #[serde(default)]
    pub messages: Vec<A2aMessage>,
    /// 产物列表
    #[serde(default)]
    pub artifacts: Vec<A2aArtifact>,
    /// 元数据
    #[serde(default)]
    pub metadata: Value,
}

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct A2aTaskStatus {
    /// 状态枚举
    pub state: A2aTaskState,
    /// 时间戳（ISO 8601）
    pub timestamp: String,
    /// 可选状态消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 任务状态枚举
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum A2aTaskState {
    /// 已提交
    Submitted,
    /// 处理中
    Working,
    /// 需要输入
    InputRequired,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已取消
    Canceled,
}

// ===== Message =====

/// A2A 消息 — 对应 ai_orz 的 MessagePo
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct A2aMessage {
    /// 角色："user" 或 "agent"
    pub role: String,
    /// 消息内容部分
    pub parts: Vec<A2aMessagePart>,
    /// 消息 ID（= ai_orz message id）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// 关联 Task ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// 消息内容部分
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum A2aMessagePart {
    /// 文本
    Text {
        /// 文本内容
        text: String,
    },
    /// 文件
    File {
        /// 文件信息
        file: A2aFilePart,
    },
    /// 结构化数据
    Data {
        /// 数据内容
        data: Value,
    },
}

/// 文件部分
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct A2aFilePart {
    /// 文件名
    pub name: String,
    /// MIME 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// base64 编码内容
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>,
    /// 文件 URI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

// ===== Artifact =====

/// 任务产物
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct A2aArtifact {
    /// 产物 ID（= ai_orz artifact id）
    pub artifact_id: String,
    /// 产物名称
    pub name: String,
    /// 内容部分
    pub parts: Vec<A2aMessagePart>,
}

// ===== 方法参数 =====

/// `tasks/send` 请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTaskParams {
    /// 客户端生成的 task id（ai_orz 忽略，使用自己生成的 project id）
    pub id: String,
    /// 消息内容
    pub message: A2aMessage,
    /// 会话 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// 元数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// `tasks/get` 请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskParams {
    /// Task ID
    pub id: String,
    /// 历史消息长度限制
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
}

/// `tasks/cancel` 请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskParams {
    /// Task ID
    pub id: String,
}
