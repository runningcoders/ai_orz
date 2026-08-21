//! Tavily 集成（用户级）API DTO - 前后端共享
//!
//! 路由统一挂 `/api/v1/finance/identity/tavily/`：
//! - 个人 API key 凭证 CRUD（user_credentials 独立表，key 加密落库永不回显）
//! - 默认凭证（多条 key 时 tavily_search 工具身份优先取默认）
//! - 集成状态聚合（凭证快照）

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ==================== 凭证 CRUD ====================

/// 创建 Tavily 凭证请求（手动录入个人 API key）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateTavilyCredentialRequest {
    /// 凭证名称（用户自命名，如「个人号」「团队号」）
    pub name: String,
    /// Tavily API key（落库加密，永不回显）
    pub api_key: String,
}

/// 创建 Tavily 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTavilyCredentialResponse {
    /// 凭证 ID
    pub credential_id: String,
}

/// 更新 Tavily 凭证请求（path 参数：credential_id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateTavilyCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
    /// 新名称（空白不变）
    #[serde(default)]
    pub name: Option<String>,
    /// 新 API key（空白不变，非空重新加密）
    #[serde(default)]
    pub api_key: Option<String>,
}

/// 更新 Tavily 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTavilyCredentialResponse {
    /// 是否更新成功
    pub success: bool,
}

/// 删除 Tavily 凭证请求（path 参数：credential_id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteTavilyCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
}

/// 删除 Tavily 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTavilyCredentialResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 设置默认 Tavily 凭证请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetDefaultTavilyCredentialRequest {
    /// 凭证 ID（空串表示取消默认，回退取第一条）
    pub credential_id: String,
}

/// 设置默认 Tavily 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetDefaultTavilyCredentialResponse {
    /// 是否设置成功
    pub success: bool,
}

// ==================== 状态聚合 ====================

/// Tavily 集成状态请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct TavilyIntegrationStatusRequest {}

/// Tavily 集成状态聚合响应（Settings Tavily 区块唯一数据来源）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TavilyIntegrationStatusResponse {
    /// 当前用户已绑定的 Tavily 个人 key 凭证（key 永不回显，仅尾号）
    pub credentials: Vec<TavilyCredentialSnapshot>,
}

/// 单个 Tavily 凭证快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TavilyCredentialSnapshot {
    /// 凭证 ID
    pub credential_id: String,
    /// 凭证名称
    pub name: String,
    /// API key 尾号（后 4 位，帮助区分多凭证，不构成泄露面）
    pub api_key_tail: String,
    /// 是否为 tavily_search 工具身份默认凭证
    #[serde(default)]
    pub is_default: bool,
}
