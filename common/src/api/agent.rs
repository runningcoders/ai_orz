//! Agent (AI智能体) related API request/response DTOs - shared between backend and frontend

use crate::api::{PagedResult, PaginationParams, skill::SkillListItem, tool::ToolListItem};
use crate::enums::{AgentRuntimeState, AgentStatus};
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
    /// 是否装配工具扁平列表（去重后的工具实体全集，按 id 唯一）
    ///
    /// 仅当为 `true` 时才装配 `GetAgentResponse::tool_list`；
    /// 否则该字段为 `None`，后端跳过工具查询。
    /// 该列表体积较大，调用方应仅在需要展示时请求（如 Agent 详情页）。
    #[param(source = "query")]
    pub with_tools: Option<bool>,
    /// 是否装配技能扁平列表（去重后的技能实体全集，按 id 唯一）
    ///
    /// 仅当为 `true` 时才装配 `GetAgentResponse::skill_list`；
    /// 否则该字段为 `None`，后端跳过技能查询。
    #[param(source = "query")]
    pub with_skills: Option<bool>,
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

// Agent 工具/技能全景已改为扁平去重列表（见 `GetAgentResponse.tool_list` /
// `skill_list`），前端按 installed pack tag 自行分组，后端不再维护分组视图结构。

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

/// Agent 运行时配置信息（详情页展示 / 编辑表单使用）
///
/// 对应 `AgentRuntimeConfig` 中可由用户在 UI 配置的字段子集。
/// 其余字段（如 installed_tags / external_config 等）由其他流程管理，不在本结构体暴露。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentRuntimeConfigInfo {
    /// 单次唤醒最大思考轮次（0 = 使用系统配置）
    pub max_thinking_rounds: usize,
    /// 意图识别阶段最大思考轮次（0 = 使用系统配置）
    pub intent_analyze_max_rounds: usize,
    /// 总结退出阶段最大思考轮次（0 = 使用系统配置）
    pub summary_max_rounds: usize,
    /// 思考超时秒数（0 = 不限制）
    pub think_timeout_secs: u64,
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
    /// 运行时配置（思考轮次 / 超时等可调参数）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<AgentRuntimeConfigInfo>,
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
    /// 当前关联的任务 ID（仅忙碌时有效）
    pub current_task_id: Option<String>,
    /// 当前关联的项目 ID（仅忙碌时有效）
    pub current_project_id: Option<String>,
    /// Agent 已拥有并去重的工具列表（扁平，按 id 唯一）；前端按 installed pack tag 自行分组
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_list: Option<Vec<ToolListItem>>,
    /// Agent 已安装并去重的技能列表（扁平，按 id 唯一，均为 Agent 自身目录下的副本）；前端自行分组
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_list: Option<Vec<SkillListItem>>,
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
    /// 运行时配置（思考轮次 / 超时等可调参数），整体替换
    pub runtime_config: Option<AgentRuntimeConfigInfo>,
}

/// 更新 Agent 状态请求
///
/// 保持严格反序列化（不设 serde(default) 兜底）：缺失字段会报错，
/// 暴露前端调用漏字段的问题。前端统一用该 DTO 完整拼接 body（含 id）。
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
    /// 角色标签列表
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
    /// 运行时配置（思考轮次 / 超时等可调参数）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<AgentRuntimeConfigInfo>,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 删除 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteAgentResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 获取 Agent 列表请求（语法糖：只接受分页参数，内部固定排除 Deleted + created_at DESC）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListAgentsRequest {
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
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
    /// 运行时状态筛选（0=Idle, 1=Resting, 2=Busy）
    pub runtime_state: Option<AgentRuntimeState>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Agent 列表项响应别名（前端兼容）
pub type ListAgentsResponseItem = AgentListItem;

/// 搜索 Agent 请求（POST body，支持完整过滤条件 + 关键词搜索）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchAgentsRequest {
    /// 搜索关键词（支持 FTS5 全文搜索 + 向量语义搜索）
    pub keyword: Option<String>,
    /// 状态筛选
    pub status: Option<AgentStatus>,
    /// 创建者 ID
    pub created_by: Option<String>,
    /// 模型供应商 ID
    pub model_provider_id: Option<String>,
    /// 角色列表
    pub roles: Option<Vec<String>>,
    /// 运行时状态筛选（0=Idle, 1=Resting, 2=Busy）
    pub runtime_state: Option<AgentRuntimeState>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// 搜索 Agent 响应（分页）
pub type SearchAgentsResponse = PagedResult<AgentListItem>;

/// 更新 Agent 状态响应
pub type UpdateAgentStatusResponse = GetAgentResponse;

/// 安装工具包请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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

/// 同步 Agent 包请求
///
/// 通用恢复/同步入口，负责两件事：
/// 1. 基础包缺失补装：为 Agent 补齐 neural / skill_management / tool_management
///    三个基础工具包与技能包（工具包只是关联关系，无包内补全问题）；
/// 2. 已安装技能包增量补全：检测已安装技能包下是否有 Agent 尚未拥有的新增已发布技能，
///    有则重装该技能包补全缺失（同时顺带刷新已有副本内容）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SyncAgentPacksRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
}

/// 同步 Agent 包响应
///
/// 全程幂等：已存在的关联/副本不会重复创建，仅返回本次实际发生变更的 tags。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SyncAgentPacksResponse {
    /// Agent ID
    pub agent_id: String,
    /// 本次补装的基础工具包 tags（此前缺失，现已就位）
    pub installed_tool_tags: Vec<String>,
    /// 本次补装的基础技能包 tags（此前缺失，现已就位）
    pub installed_skill_packs: Vec<String>,
    /// 因检测到新增已发布技能而重装补全的技能包 tags（含基础技能包）
    pub refreshed_skill_packs: Vec<String>,
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

/// Agent 匹配规则（打分 resolve 的查询参数）
///
/// 字段都是 Option，传 None 表示该维度不参与打分。
/// 所有维度加完分后按总分 DESC、created_at ASC 取第 1 名。
/// 如果全体候选均 0 分（没命中任何条件），退化回取任意 Onboarded（created_at 最早）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct AgentMatchCriteria {
    /// 指定 Agent ID 集合：直查短路，命中即决定性胜出（权重远高于其他维度，
    /// 见 match_scores::ID_EXACT_MATCH）。用于「渠道显式绑定」等必中场景。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any_id: Option<Vec<String>>,

    /// 角色标签渐进匹配：优先全匹配 → 部分精确 → 子串层级 → 语义（可选）。
    /// 命中即加高分；不要求全中（渐进式，命中越多越靠前）。
    /// 典型用：["reception"] 选 Web 前台、["feishu_reception"] 选飞书前台。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any_role: Option<Vec<String>>,

    /// 能力关键词（弱参与）：在 agent.capabilities 的每条能力中做子串匹配，
    /// 命中几条加几分。传 None 或空字符串 = 忽略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,

    /// 是否启用向量语义兜底（默认 false）。
    /// true 时：当字符串层（全/部分/子串）均无命中，用 agent 向量索引做语义相似度兜底。
    /// 依赖系统已配置 Embedding Provider；未配置时静默跳过该维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_fallback: Option<bool>,

    /// 候选集拉取数量（默认 10）。打分前用于限制 DAL 查询范围，
    /// 避免把整个组织的 Agent 都拉到内存里算分。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_limit: Option<usize>,
}

impl AgentMatchCriteria {
    /// 空条件 = 所有维度忽略，等价于退化回"任意 Onboarded"。
    pub fn any() -> Self {
        Self::default()
    }

    /// 常用快捷构造：指定单个 Agent（决定性命中，直查短路）
    pub fn by_id(id: impl Into<String>) -> Self {
        Self {
            any_id: Some(vec![id.into()]),
            ..Default::default()
        }
    }

    /// 常用快捷构造：指定多个 Agent 择一（决定性命中，直查短路）
    pub fn by_ids(ids: Vec<String>) -> Self {
        if ids.is_empty() {
            Self::default()
        } else {
            Self {
                any_id: Some(ids),
                ..Default::default()
            }
        }
    }

    /// 常用快捷构造：按任意一个命中的角色去匹配（含语义兜底）
    pub fn by_role(role: impl Into<String>) -> Self {
        Self {
            any_role: Some(vec![role.into()]),
            semantic_fallback: Some(true),
            ..Default::default()
        }
    }

    /// 常用快捷构造：多角色择一匹配（含语义兜底）
    pub fn by_roles(roles: Vec<String>) -> Self {
        if roles.is_empty() {
            Self::default()
        } else {
            Self {
                any_role: Some(roles),
                semantic_fallback: Some(true),
                ..Default::default()
            }
        }
    }
}

/// Agent 匹配打分权重（集中在一处，方便以后调整）。
///
/// 总分 = 渐进角色匹配分 + capability_keyword_hit × 10 + installed_tag_hit × 3
/// 渐进角色匹配：全匹配（tier1）> 部分精确（tier2）> 子串层级（tier3）> 语义（tier4）
pub mod match_scores {
    /// id 精确命中（决定性）。权重量级隔离：需要 100 次 role 全匹配才能追平，
    /// 保证「渠道显式绑定」这类指定关系不被任何弱维度叠加翻盘。
    pub const ID_EXACT_MATCH: i32 = 1_000_000;

    /// 全匹配：criteria 所有角色都被 agent.roles 精确命中（tier1）
    pub const ROLE_FULL_MATCH: i32 = 10000;

    /// 部分精确：至少一个角色精确命中（tier2）
    pub const ROLE_PARTIAL_MATCH: i32 = 5000;

    /// 子串层级：角色名存在包含/被包含关系（如 `feishu_reception` ⊃ `reception`，tier3）
    pub const ROLE_SUBSTRING_MATCH: i32 = 1000;

    /// 语义匹配：无字符串命中，但向量语义相近（tier4）
    pub const ROLE_SEMANTIC_MATCH: i32 = 200;

    /// capabilities 里关键词每匹配到一条能力 + 的分
    pub const CAPABILITY_PER_HIT: i32 = 10;

    /// installed_tags（工具包 tags）每命中一条 + 的分（弱）
    pub const INSTALLED_TAG_PER_HIT: i32 = 3;
}
