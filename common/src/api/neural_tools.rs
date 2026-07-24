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
    /// 图谱遍历深度，默认0=不遍历。
    pub traversal_depth: Option<i32>,
    /// 每层展开广度，默认0=不限制。
    pub traversal_breadth: Option<i32>,
    /// 遍历策略：breadth_first / depth_first。
    pub traversal_strategy: Option<String>,
    /// 种子节点ID列表，跳过语义搜索直接遍历。
    pub seed_node_ids: Option<Vec<String>>,
    /// 标签过滤（OR 语义，命中任一 tag 即可）。
    pub tags: Option<Vec<String>>,
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
    /// 关系类型：源节点 ID（仅 relation 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<String>,
    /// 关系类型：目标节点 ID（仅 relation 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<String>,
    /// 关系类型名称（仅 relation 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_type: Option<String>,
    /// 标签列表（仅 short_term / knowledge_node 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
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
    /// 标签过滤（OR 语义，命中任一 tag 即可）。
    pub tags: Option<Vec<String>>,
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

/// 发送消息给 Agent 请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SendMessageToAgentParams {
    /// 接收 Agent ID（可选）
    ///
    /// 协作关系类比：
    /// - 默认对话框：用户选定 Agent 时传，未选定时为 None（后端走 resolve_agent 兜底）
    /// - Project 对话框：从 project.owner_agent_id 取；若为 None 也走 resolve_agent 兜底
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_agent_id: Option<String>,
    /// 消息内容。
    pub content: String,
    /// 关联项目 ID（默认对话框场景为 None）。
    pub project_id: Option<String>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
    /// 回复的消息 ID。
    pub reply_to_id: Option<String>,
    /// 附件 ID 列表。
    /// 发送方已经上传到 Attachment 模块的附件 ID 列表，
    /// 后端会为每个附件创建一条附件消息（Image/File/Audio/Video），
    /// 紧跟在文本消息之前（按数组顺序排列）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attachment_ids: Option<Vec<String>>,
}

/// 发送消息给 Agent 响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SendMessageToAgentResponse {
    /// 消息 ID。
    pub message_id: String,
}

/// 请求工具调用参数（同步）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct RequestToolCallParams {
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

/// 请求工具调用响应（同步）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RequestToolCallResponse {
    /// 工具调用 ID。
    pub tool_call_id: String,
    /// 调用状态。
    pub status: String,
    /// 工具执行结果。
    pub result: serde_json::Value,
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

/// 知识图谱关联关系参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct KnowledgeRelationParam {
    /// 源节点 ID。
    pub source_node_id: String,
    /// 目标节点 ID。
    pub target_node_id: String,
    /// 关系类型。
    pub relation_type: String,
}

/// 保存短期记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SaveShortTermMemoryParams {
    /// 记忆摘要。
    pub summary: String,
    /// 标签列表。
    pub tags: Option<Vec<String>>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
    /// 详细内容，可选。
    pub content: Option<String>,
}

/// 保存短期记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SaveShortTermMemoryResponse {
    /// 记忆 ID。
    pub memory_id: String,
}

/// 保存长期记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SaveLongTermMemoryParams {
    /// 节点名称。
    pub node_name: String,
    /// 节点描述。
    pub node_description: String,
    /// 节点类型，如 concept/fact/skill/pattern。
    pub node_type: String,
    /// 节点摘要。
    pub summary: Option<String>,
    /// 标签列表（用于过滤检索 + 全文索引）。
    pub tags: Option<Vec<String>>,
    /// 关联关系列表。
    pub relations: Option<Vec<KnowledgeRelationParam>>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
}

/// 保存长期记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SaveLongTermMemoryResponse {
    /// 节点 ID。
    pub node_id: String,
    /// 创建的关系 ID 列表，可能为空。
    pub relation_ids: Vec<String>,
}

/// 沉淀记忆请求参数。
///
/// 将未沉淀的短期记忆总结并沉淀为长期知识图谱。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SettleMemoryParams {
    /// 每次处理的短期记忆数量上限，默认 10。
    pub limit: Option<usize>,
}

/// 沉淀记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SettleMemoryResponse {
    /// 沉淀创建的知识节点数量。
    pub settled_count: usize,
}

/// 搜索技能请求参数。
///
/// 按关键词或标签搜索技能库，返回技能摘要列表（不含完整内容）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SearchSkillParams {
    /// 搜索关键词（匹配技能名称、描述、tags）。
    pub keyword: Option<String>,
    /// 按 tag 过滤（OR 语义，命中任一即可）。
    pub tags: Option<Vec<String>>,
    /// 返回数量限制，默认 10。
    pub limit: Option<usize>,
}

/// 搜索技能响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SearchSkillResponse {
    /// 搜索结果列表。
    pub skills: Vec<SkillSummary>,
}

/// 技能摘要（不含完整内容，用于搜索/列表展示）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SkillSummary {
    /// 技能 ID。
    pub skill_id: String,
    /// 技能名称。
    pub name: String,
    /// 技能描述。
    pub description: String,
    /// 标签列表。
    pub tags: Vec<String>,
}
