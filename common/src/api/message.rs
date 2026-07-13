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
}

/// 消息列表响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListMessagesResponse {
    /// 消息列表（按 created_at ASC 排序）
    pub messages: Vec<MessageListItem>,
    /// 总数（当前页条数）
    pub total: usize,
}
