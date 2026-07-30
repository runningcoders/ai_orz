//! Organization-related API request/response DTOs - shared between backend and frontend

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 系统初始化请求 - 创建第一个组织和超级管理员
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct InitializeSystemRequest {
    /// 组织名称
    pub organization_name: String,
    /// 超级管理员用户名
    pub admin_username: String,
    /// 超级管理员密码（前端已哈希）
    pub admin_password_hash: String,
    /// 组织描述（可选）
    pub description: Option<String>,
    /// 超级管理员显示名称（可选）
    pub admin_display_name: Option<String>,
    /// 超级管理员邮箱（可选）
    pub admin_email: Option<String>,
    /// 对话模型配置（用于 Agent 思考和对话）
    pub chat_model: ModelProviderInitConfig,
    /// 向量模型配置（用于 Embedding 向量化，可选 — 不传时跳过向量索引）
    #[serde(default)]
    pub embedding_model: Option<ModelProviderInitConfig>,
}

/// 模型 Provider 初始化配置（系统初始化时使用）
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ModelProviderInitConfig {
    /// Provider 名称（如 "OpenAI"、"DeepSeek"）
    pub name: String,
    /// 服务商类型（0=OpenAI, 1=DeepSeek, 2=Qwen, 3=Doubao, 4=Ollama, 5=Custom, 6=FastEmbed）
    pub provider_type: i32,
    /// 模型名称（如 "gpt-4o"、"text-embedding-3-small"）
    pub model_name: String,
    /// API Key（明文，后端存储时会加密）
    pub api_key: String,
    /// 自定义 Base URL（可选，用于 OpenAI 兼容代理）
    pub base_url: Option<String>,
    /// 描述（可选）
    pub description: Option<String>,
}

/// 检查系统初始化状态请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CheckInitializedRequest {}

/// 系统初始化响应（最终结果，进度查询完成时返回）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InitializeSystemResponse {
    /// 组织 ID
    pub organization_id: String,
    /// 超级管理员用户 ID
    pub user_id: String,
    /// 对话模型 Provider ID
    pub chat_provider_id: String,
    /// 向量模型 Provider ID（None 表示未创建向量模型）
    pub embedding_provider_id: Option<String>,
}

/// 系统初始化异步提交响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InitializeSystemAsyncResponse {
    /// 异步任务 ID（用于查询进度）
    pub task_id: String,
}

/// 初始化任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InitStatus {
    /// 排队中
    Pending,
    /// 执行中
    Running,
    /// 已完成
    Completed,
    /// 失败
    Failed,
}

/// 初始化进度查询请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetInitProgressRequest {
    /// 异步任务 ID
    #[param(source = "query")]
    pub task_id: String,
}

/// 初始化进度响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InitProgressResponse {
    /// 异步任务 ID
    pub task_id: String,
    /// 任务状态
    pub status: InitStatus,
    /// 当前步骤序号（从 1 开始）
    pub current_step: usize,
    /// 总步骤数
    pub total_steps: usize,
    /// 当前步骤描述
    pub step_message: String,
    /// 开始时间戳（秒）
    pub started_at: i64,
    /// 结束时间戳（秒，None 表示未结束）
    pub finished_at: Option<i64>,
    /// 错误信息（Failed 时有值）
    pub error: Option<String>,
    /// 初始化结果（Completed 时有值）
    pub result: Option<InitializeSystemResponse>,
}

/// 检查初始化状态响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckInitializedResponse {
    /// 系统是否已初始化（至少有一个组织）
    pub initialized: bool,
}

/// 组织列表项（用于登录页选择）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrganizationListItem {
    /// 组织 ID
    pub organization_id: String,
    /// 组织名称
    pub name: String,
    /// 组织描述（可选）
    pub description: Option<String>,
}

/// 列出所有组织响应（登录页选择用）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListOrganizationsResponse {
    /// 组织列表
    pub data: Vec<OrganizationListItem>,
    /// 总数
    pub total: u64,
}

/// 组织基础信息响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrganizationInfoResponse {
    /// 组织 ID
    pub organization_id: String,
    /// 组织名称
    pub name: String,
    /// 组织描述（可选）
    pub description: Option<String>,
    /// 外部访问 Base URL（可选）
    pub base_url: Option<String>,
    /// 组织状态（1: 活跃, 0: 非活跃）
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
}

/// 获取当前组织信息请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetCurrentOrganizationRequest {}

/// 获取当前组织信息响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetCurrentOrganizationResponse {
    /// 组织信息数据
    pub data: OrganizationInfoResponse,
}

/// 更新当前组织信息请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateCurrentOrganizationRequest {
    /// 新组织名称（None 表示不修改）
    pub name: Option<String>,
    /// 新组织描述（None 表示不修改）
    pub description: Option<String>,
    /// 新外部访问 Base URL（None 表示不修改）
    pub base_url: Option<String>,
}

/// 更新当前组织信息响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateCurrentOrganizationResponse {
    /// 更新后的组织信息
    pub data: OrganizationInfoResponse,
}

/// 获取组织信息请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetOrganizationRequest {
    /// Organization ID
    #[param(source = "path")]
    pub organization_id: String,
}

/// 获取组织信息响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetOrganizationResponse {
    /// 组织信息数据
    pub data: OrganizationInfoResponse,
}

/// 更新组织信息请求（管理员更新任意组织）
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateOrganizationRequest {
    /// Organization ID
    #[param(source = "path")]
    pub organization_id: String,
    /// 新组织名称（None 表示不修改）
    pub name: Option<String>,
    /// 新组织描述（None 表示不修改）
    pub description: Option<String>,
    /// 新外部访问 Base URL（None 表示不修改）
    pub base_url: Option<String>,
    /// 新组织状态（None 表示不修改）
    pub status: Option<i32>,
}

/// 更新组织信息响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateOrganizationResponse {
    /// 更新后的组织信息
    pub data: OrganizationInfoResponse,
}

/// 删除组织请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteOrganizationRequest {
    /// Organization ID
    #[param(source = "path")]
    pub organization_id: String,
}

/// 删除组织响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteOrganizationResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 列出所有组织请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListOrganizationsRequest {}
