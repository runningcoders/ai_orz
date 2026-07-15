//! Message API DTOs - 消息列表查询

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 消息列表查询请求（GET query params）
///
/// 支持两种分页模式：
/// 1. 初始加载 / 上拉翻页：`before_timestamp` + `limit` + `order=desc` → 获取更早的消息
/// 2. 下拉轮询新消息：`after_timestamp` → 获取更新的消息
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListMessagesRequest {
    /// 按项目 ID 过滤
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// 按任务 ID 过滤
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 按发送方 ID 过滤
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_id: Option<String>,
    /// 按接收方 ID 过滤
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_id: Option<String>,
    /// 上拉翻页：只返回 created_at 小于此值的消息（毫秒时间戳）
    /// 用于加载更早的历史消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_timestamp: Option<i64>,
    /// 下拉轮询：只返回 created_at 大于此值的消息（毫秒时间戳）
    /// 用于增量拉取新消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_timestamp: Option<i64>,
    /// 限制返回条数（默认 10）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// 消息列表项（脱敏后的展示对象）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MessageListItem {
    /// 消息 ID
    pub message_id: String,
    /// 关联项目 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// 关联任务 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 发送方 ID
    pub from_id: String,
    /// 发送方角色（0=User, 1=Agent, 2=System）
    pub from_role: i32,
    /// 接收方 ID
    pub to_id: String,
    /// 接收方角色
    pub to_role: i32,
    /// 消息类型（0=Text, 5=ToolCallRequest, 6=ToolCallResult, 9=TaskAssignment 等）
    pub message_type: i32,
    /// 消息状态（0=Recalled, 1=Pending, 2=Processing, 3=Processed, 4=Failed）
    pub status: i32,
    /// 消息内容
    pub content: String,
    /// 回复的消息 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_id: Option<String>,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 文件类型（附件消息才有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<i32>,
    /// 文件元数据（附件消息才有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_meta: Option<FileMetaInfo>,
}

/// 文件元数据信息（用于消息附件展示）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileMetaInfo {
    /// 文件名
    pub name: String,
    /// MIME 类型
    pub mime_type: String,
    /// 文件大小（字节）
    pub size: u64,
}

/// 消息列表响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListMessagesResponse {
    /// 消息列表（按 created_at ASC 排序）
    pub messages: Vec<MessageListItem>,
    /// 总数（当前页条数）
    pub total: usize,
}

/// 消息搜索请求（POST body）
///
/// 支持混合搜索：关键词搜索 + 向量语义搜索 + 业务过滤
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchMessagesRequest {
    /// 搜索关键词（FTS5 全文检索）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
    /// 按项目 ID 过滤
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// 按任务 ID 过滤
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 按发送方 ID 过滤
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_id: Option<String>,
    /// 按接收方 ID 过滤
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_id: Option<String>,
    /// 返回数量限制（默认 20）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// 消息搜索响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SearchMessagesResponse {
    /// 搜索结果列表（按相关性排序）
    pub messages: Vec<MessageSearchResult>,
    /// 总匹配数
    pub total: usize,
}

/// 消息搜索结果项（包含匹配信息）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MessageSearchResult {
    /// 消息 ID
    pub message_id: String,
    /// 关联项目 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// 关联任务 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 发送方 ID
    pub from_id: String,
    /// 发送方角色
    pub from_role: i32,
    /// 接收方 ID
    pub to_id: String,
    /// 接收方角色
    pub to_role: i32,
    /// 消息类型
    pub message_type: i32,
    /// 消息内容（截断显示）
    pub content: String,
    /// 创建时间戳
    pub created_at: i64,
    /// 匹配类型：hybrid/vector/keyword
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_type: Option<String>,
    /// FTS5 相关性分数（越小越相关）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fts_rank: Option<f32>,
    /// 向量相似度距离（越小越相似）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_distance: Option<f32>,
}

// ==================== 工具调用消息结构 ====================

/// 工具调用消息内容
///
/// 对应 MessageType::ToolCallRequest 或 MessageType::ToolCallResult
/// 存储在 message.content 字段中的 JSON 结构
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallMessagePayload {
    /// 工具调用请求 ID
    pub request_id: String,
    /// 工具 ID
    pub tool_id: String,
    /// 工具名称
    pub tool_name: String,
    /// 关联项目 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// 关联任务 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 发起方 ID
    pub from_id: String,
    /// 目标执行方 ID
    pub to_id: String,
    /// 调用参数（请求时有效）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
    /// 调用结果（完成后有效）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// 是否执行成功（结果时有效）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_success: Option<bool>,
    /// 错误信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

// ==================== 任务分配消息结构 ====================

/// 任务分配消息内容
///
/// 对应 MessageType::TaskAssignment
/// 存储在 message.content 字段中的 JSON 结构
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TaskAssignmentMessagePayload {
    /// 任务 ID
    pub task_id: String,
    /// 任务标题
    pub task_title: String,
    /// 任务描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_description: Option<String>,
    /// 关联项目 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// 分配者 ID
    pub from_id: String,
    /// 接收 Agent ID
    pub to_agent_id: String,
}
