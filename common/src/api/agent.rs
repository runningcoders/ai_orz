//! Agent (AI智能体) related API request/response DTOs - shared between backend and frontend

use crate::api::PaginationParams;
use crate::enums::AgentStatus;
use crate::models::{AgentStats, ModelCallStats};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 创建 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateAgentRequest {
    /// Agent 名称
    pub name: String,
    /// Agent 角色标签列表
    pub roles: Option<Vec<String>>,
    /// Agent 描述
    pub description: Option<String>,
    /// 能力列表
    pub capabilities: Option<Vec<String>>,
    /// Agent 灵魂提示词
    pub soul: Option<String>,
    /// 关联的模型提供商 ID
    pub model_provider_id: String,
}

/// 创建 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateAgentResponse {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// Agent 描述
    pub description: Option<String>,
    /// 创建时间戳
    pub created_at: i64,
}

/// Agent 列表项响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentListItem {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// Agent 角色标签列表
    pub roles: Vec<String>,
    /// Agent 描述
    pub description: Option<String>,
    /// Agent 类型：local / cli / remote
    pub kind: String,
    /// 关联的模型提供商 ID（仅 local 类型有值）
    pub model_provider_id: String,
    /// 生命周期状态
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
    /// 运行时状态（内存状态）
    pub runtime_state: i32,
}

/// 从详情响应构造列表项（用于按需加载场景，避免全量 list_agents）
impl From<&GetAgentResponse> for AgentListItem {
    fn from(a: &GetAgentResponse) -> Self {
        Self {
            id: a.id.clone(),
            name: a.name.clone(),
            roles: a.roles.clone(),
            description: a.description.clone(),
            kind: a.kind.clone(),
            model_provider_id: a.model_provider_id.clone(),
            status: a.status,
            created_at: a.created_at,
            runtime_state: a.runtime_state,
        }
    }
}

/// 获取 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,
    /// 是否加载统计信息（唤醒次数汇总）
    #[param(source = "query")]
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（token + 时序趋势）
    #[param(source = "query")]
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围起始（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_start: Option<i64>,
    /// 统计时间范围结束（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_end: Option<i64>,
    /// 时序查询粒度：hourly / daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,
}

/// 外部 Agent 配置信息（详情页展示用）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentExternalConfigInfo {
    /// CLI 配置（kind=cli 时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<AgentCliConfig>,
    /// Remote 配置（kind=remote 时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<AgentRemoteConfig>,
}

/// CLI 外部 Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentCliConfig {
    /// 启动命令
    pub command: String,
    /// 命令参数
    #[serde(default)]
    pub args: Vec<String>,
    /// 工作目录
    pub work_dir: String,
    /// 超时时间（秒）
    pub timeout_secs: u64,
    /// 自定义 prompt 模板
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_template: Option<String>,
}

/// Remote 外部 Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentRemoteConfig {
    /// A2A Server 的 base URL
    pub endpoint: String,
    /// 目标 Agent 名称
    pub agent_name: String,
    /// 超时时间（秒）
    pub timeout_secs: u64,
}

/// 获取 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetAgentResponse {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// Agent 角色标签列表
    pub roles: Vec<String>,
    /// Agent 描述
    pub description: Option<String>,
    /// 能力列表
    pub capabilities: Option<Vec<String>>,
    /// 灵魂提示词
    pub soul: Option<String>,
    /// Agent 类型：local / cli / remote
    pub kind: String,
    /// 关联的模型提供商 ID（仅 local 类型有值）
    pub model_provider_id: String,
    /// 外部 Agent 配置（仅 cli/remote 类型有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_config: Option<AgentExternalConfigInfo>,
    /// 生命周期状态
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
    /// 运行时状态（内存状态，服务重启后重置）
    pub runtime_state: i32,
    /// 当前处理的消息 ID（仅忙碌时有效）
    pub current_message_id: Option<String>,
    /// 已绑定的工具 ID 列表
    pub tools: Vec<String>,
    /// Agent 自身统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<AgentStats>,
    /// 模型调用统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_call_stats: Option<ModelCallStats>,
}

/// 更新 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,

    /// Agent 名称
    pub name: Option<String>,
    /// Agent 角色标签列表
    pub roles: Option<Vec<String>>,
    /// Agent 描述
    pub description: Option<String>,
    /// 能力列表
    pub capabilities: Option<Vec<String>>,
    /// Agent 灵魂提示词
    pub soul: Option<String>,
    /// 关联的模型提供商 ID
    pub model_provider_id: Option<String>,
}

/// 更新 Agent 状态请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateAgentStatusRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,

    /// 目标生命周期状态。
    ///
    /// 状态流转合法性由 HR Domain 校验；删除请优先使用 DELETE 接口。
    pub status: AgentStatus,
}

/// 删除 Agent 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,
}

/// 更新 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateAgentResponse {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// Agent 描述
    pub description: Option<String>,
    /// 能力列表
    pub capabilities: Option<Vec<String>>,
    /// 灵魂提示词
    pub soul: Option<String>,
    /// Agent 类型：local / cli / remote
    pub kind: String,
    /// 关联的模型提供商 ID（仅 local 类型有值）
    pub model_provider_id: String,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 删除 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteAgentResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 获取 Agent 列表请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListAgentsRequest {
    /// 可选状态筛选
    #[param(source = "query")]
    pub status: Option<AgentStatus>,
    /// 按 ID 批量查询
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}

/// 获取 Agent 列表响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListAgentsResponse {
    /// Agent 列表
    pub agents: Vec<AgentListItem>,
}

/// Agent 通用查询请求（POST body，支持完整查询能力）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct AgentQueryRequest {
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 关键词搜索（名称/描述）
    pub keyword: Option<String>,
    /// 状态筛选
    pub status: Option<AgentStatus>,
    /// 创建者 ID
    pub created_by: Option<String>,
    /// 模型供应商 ID
    pub model_provider_id: Option<String>,
    /// 角色列表
    pub roles: Option<Vec<String>>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Agent 列表项响应别名（前端兼容）
pub type ListAgentsResponseItem = AgentListItem;

/// 搜索 Agent 请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SearchAgentsRequest {
    /// 搜索关键词（支持 FTS5 全文搜索）
    #[param(source = "query")]
    pub keyword: Option<String>,
    /// 返回结果数量限制
    #[param(source = "query")]
    pub limit: Option<usize>,
}

/// 搜索 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchAgentsResponse {
    /// Agent 列表
    pub agents: Vec<AgentListItem>,
}

/// 更新 Agent 状态响应
pub type UpdateAgentStatusResponse = GetAgentResponse;

/// 安装工具包请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct InstallToolPackRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,

    /// 工具包 tag（如 "project_management"）
    #[param(source = "path")]
    pub tag: String,
}

/// 安装工具包响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstallToolPackResponse {
    /// Agent ID
    pub agent_id: String,
    /// 已安装的工具包 tags
    pub installed_tags: Vec<String>,
}

/// 卸载工具包请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UninstallToolPackRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,

    /// 工具包 tag
    #[param(source = "path")]
    pub tag: String,
}

/// 卸载工具包响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UninstallToolPackResponse {
    /// Agent ID
    pub agent_id: String,
    /// 剩余已安装的工具包 tags
    pub installed_tags: Vec<String>,
}

/// 列出已安装工具包请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListInstalledToolPacksRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
}

/// 列出已安装工具包响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListInstalledToolPacksResponse {
    /// Agent ID
    pub agent_id: String,
    /// 已安装的工具包 tags
    pub installed_tags: Vec<String>,
}

/// 查询前台 Agent 请求
///
/// 无参数 — 后端通过 `HrDomain::resolve_agent(ctx)` 路由到当前可用的前台 Agent。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetReceptionAgentRequest {}

/// 查询前台 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetReceptionAgentResponse {
    /// 前台 Agent ID
    pub agent_id: String,
    /// 前台 Agent 名称
    pub agent_name: String,
}
