//! Neural tools API request/response DTOs - shared between backend and frontend

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 搜索记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SearchMemoryParams {
    /// 搜索关键词。
    pub query: String,
    /// 返回最大结果数。
    pub max_results: Option<i32>,
    /// 记忆类型筛选。
    pub memory_type: Option<String>,
}

/// 搜索记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SearchMemoryResponse {
    /// 搜索结果列表。
    pub results: Vec<MemoryResult>,
}

/// 单条记忆结果。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct MemoryResult {
    /// 记忆 ID。
    pub id: String,
    /// 记忆内容。
    pub content: String,
    /// 记忆类型。
    pub memory_type: String,
    /// 匹配分数。
    pub score: Option<f32>,
    /// 记忆摘要。
    pub summary: Option<String>,
}

/// 查询记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct QueryMemoryParams {
    /// Agent ID 筛选。
    pub agent_id: Option<String>,
    /// 记忆类型筛选。
    pub memory_type: Option<String>,
    /// 返回数量限制。
    pub limit: Option<i32>,
}

/// 查询记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct QueryMemoryResponse {
    /// 查询结果列表。
    pub results: Vec<MemoryResult>,
}

/// 创建记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateMemoryParams {
    /// 记忆类型。
    pub memory_type: String,
    /// 记忆内容。
    pub content: String,
    /// 记忆摘要。
    pub summary: Option<String>,
    /// 标签列表。
    pub tags: Option<Vec<String>>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
}

/// 创建记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreateMemoryResponse {
    /// 新建记忆的 ID。
    pub memory_id: String,
}

/// 更新记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateMemoryParams {
    /// 记忆 ID。
    pub memory_id: String,
    /// 更新内容。
    pub content: Option<String>,
    /// 更新摘要。
    pub summary: Option<String>,
    /// 更新标签。
    pub tags: Option<Vec<String>>,
}

/// 更新记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct UpdateMemoryResponse {
    /// 记忆 ID。
    pub memory_id: String,
}

/// 删除记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteMemoryParams {
    /// 记忆 ID。
    pub memory_id: String,
}

/// 删除记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct DeleteMemoryResponse {
    /// 记忆 ID。
    pub memory_id: String,
}

/// 发送消息请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SendMessageParams {
    /// 接收用户 ID。
    pub to_user_id: String,
    /// 消息内容。
    pub content: String,
    /// 关联项目 ID。
    pub project_id: Option<String>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
    /// 回复的消息 ID。
    pub reply_to_id: Option<String>,
}

/// 发送消息响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SendMessageResponse {
    /// 消息 ID。
    pub message_id: String,
}

/// 请求工具调用参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct RequestToolCallParams {
    /// 工具 ID。
    pub tool_id: String,
    /// 工具调用参数。
    pub params: serde_json::Value,
    /// 关联任务 ID。
    pub task_id: Option<String>,
}

/// 请求工具调用响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RequestToolCallResponse {
    /// 工具调用 ID。
    pub tool_call_id: String,
    /// 调用状态。
    pub status: String,
}

/// 发送工具调用消息参数（异步）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SendToolCallMessageParams {
    /// 工具 ID。
    pub tool_id: String,
    /// 工具名称。
    pub tool_name: String,
    /// 工具调用参数。
    pub params: serde_json::Value,
    /// 关联项目 ID。
    pub project_id: Option<String>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
}

/// 发送工具调用消息响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SendToolCallMessageResponse {
    /// 请求 ID。
    pub request_id: String,
    /// 消息 ID。
    pub message_id: String,
    /// 派发状态。
    pub status: String,
}

/// 标记完成请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct MarkDoneParams {
    /// 任务 ID。
    pub task_id: String,
    /// 完成摘要。
    pub summary: Option<String>,
}

/// 标记完成响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct MarkDoneResponse {
    /// 任务 ID。
    pub task_id: String,
    /// 任务状态。
    pub status: String,
}

/// 发送任务分配消息参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SendTaskAssignmentMessageParams {
    /// 任务 ID。
    pub task_id: String,
    /// 任务标题。
    pub task_title: String,
    /// 任务描述。
    pub task_description: Option<String>,
    /// 接收 Agent ID。
    pub to_agent_id: String,
    /// 关联项目 ID。
    pub project_id: Option<String>,
}

/// 发送任务分配消息响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SendTaskAssignmentMessageResponse {
    /// 消息 ID。
    pub message_id: String,
}
