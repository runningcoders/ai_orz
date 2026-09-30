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
    #[param(source = "query")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// 按任务 ID 过滤
    #[param(source = "query")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 按发送方 ID 过滤
    #[param(source = "query")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_id: Option<String>,
    /// 按接收方 ID 过滤
    #[param(source = "query")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_id: Option<String>,
    /// 按消息链根 ID 过滤（拉取整条消息链 / 话题讨论区）
    #[param(source = "query")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    /// 上拉翻页：只返回 created_at 小于此值的消息（毫秒时间戳）
    /// 用于加载更早的历史消息
    #[param(source = "query")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_timestamp: Option<i64>,
    /// 下拉轮询：只返回 created_at 大于此值的消息（毫秒时间戳）
    /// 用于增量拉取新消息
    #[param(source = "query")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_timestamp: Option<i64>,
    /// 限制返回条数（默认 10）
    #[param(source = "query")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// 按处理状态过滤（0=已撤回 / 1=待处理 / 2=处理中 / 3=处理完成 / 4=处理失败）
    ///
    /// 不传 = 不过滤（默认排除已撤回）。传 `1`（Pending）可精确拉出「尚未处理」的消息
    /// —— 它同时覆盖「排队中」与「正在处理」：`Processing` 无写入路径，在飞消息在库中
    /// 仍是 `Pending`。用于后台巡检「某 Agent / 某项目还有哪些活没干」。
    #[param(source = "query")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
}

/// 消息列表项（脱敏后的展示对象）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// 消息链根消息 ID（同一话题讨论区的所有消息共享同一 root_id）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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

// ==================== 消息撤回 ====================

/// 撤回消息请求（POST body；REST 与神经工具 `recall_message` 共用同一个 handler）
///
/// 语义边界（与工具描述同源，避免模型误承诺）：
/// - 只对**未处理（`Pending`）**的消息有效；正在处理（in-flight）的消息会**尽力取消**
///   （轮次边界生效，不保证立即停止当前 LLM 调用）。
/// - 已处理 / 已失败的消息撤回是 **no-op**（`outcome = not_recallable`，不算错误）。
/// - 撤回是「阻止后续消费」，**不是时间倒流**：不消除该消息对已读上下文的影响，且不可撤销。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RecallMessageRequest {
    /// 目标消息 ID
    pub message_id: String,
    /// 撤回原因（可选，仅用于审计日志，不落库）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 撤回消息响应
///
/// 幂等语义统一放在 `outcome` **字段**里，不靠错误码区分（撤回已/不可撤回的消息不算失败）：
/// - `recalled`：本次成功撤回（未处理 → 标撤回；在飞 → 已发出取消信号）
/// - `already_recalled`：此前已撤回（幂等成功，不报 error）
/// - `not_recallable`：消息已处理 / 已失败，撤回无意义（no-op，不报 error）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecallMessageResponse {
    /// 本次是否产生了实际效果（`outcome = recalled` 时为 true；其余为 false 但请求仍算成功）
    pub success: bool,
    /// 结果码：`recalled` / `already_recalled` / `not_recallable`
    pub outcome: String,
    /// 人类可读说明（会原样回灌给模型，须点名具体原因）
    pub message: String,
    /// 被取消的在飞 Agent ID（仅「撤回正在处理的消息」时有值）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_agent_id: Option<String>,
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
