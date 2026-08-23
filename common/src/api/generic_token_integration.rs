//! 通用 API Token 集成（用户级）API DTO - 前后端共享
//!
//! 路由统一挂 `/api/v1/finance/identity/generic-token/`：
//! - 个人 API token 凭证 CRUD（user_credentials 独立表，token 加密落库永不回显）
//! - 默认凭证（同 platform 下多条 token 时工具身份优先取默认）
//! - 集成状态聚合（按 platform 维度返回凭证快照）
//!
//! 适用于所有「单字段 API Key」类平台（如 tavily、doubao_search、未来新增平台等），
//! 通过 platform 字段二元匹配 `(CredentialKind::GenericToken, platform)`。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ==================== 凭证 CRUD ====================

/// 创建通用 API Token 凭证请求（手动录入个人 API token）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateGenericTokenCredentialRequest {
    /// 凭证名称（用户自命名，如「个人号」「团队号」）
    pub name: String,
    /// 平台标识（与工具声明的 platform 二元匹配，如 "tavily"、"doubao_search"）
    pub platform: String,
    /// API token（落库加密，永不回显）
    pub api_token: String,
}

/// 创建通用 API Token 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGenericTokenCredentialResponse {
    /// 凭证 ID
    pub credential_id: String,
}

/// 更新通用 API Token 凭证请求（path 参数：id）
///
/// platform 由凭证 ID 唯一确定，无需重复传入。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateGenericTokenCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
    /// 新名称（空白不变）
    #[serde(default)]
    pub name: Option<String>,
    /// 新 API token（空白不变，非空重新加密）
    #[serde(default)]
    pub api_token: Option<String>,
}

/// 更新通用 API Token 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateGenericTokenCredentialResponse {
    /// 是否更新成功
    pub success: bool,
}

/// 删除通用 API Token 凭证请求（path 参数：id）
///
/// platform 由凭证 ID 唯一确定，无需重复传入。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteGenericTokenCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
}

/// 删除通用 API Token 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteGenericTokenCredentialResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 设置默认通用 API Token 凭证请求
///
/// 默认作用域按 (kind=GenericToken, platform) 隔离，因此 platform 必填；
/// credential_id 空串表示取消该 platform 下的默认，回退取第一条。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetDefaultGenericTokenCredentialRequest {
    /// 平台标识
    pub platform: String,
    /// 凭证 ID（空串表示取消默认）
    pub credential_id: String,
}

/// 设置默认通用 API Token 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetDefaultGenericTokenCredentialResponse {
    /// 是否设置成功
    pub success: bool,
}

// ==================== 状态聚合 ====================

/// 通用 API Token 集成状态请求（query 参数：platform）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GenericTokenIntegrationStatusRequest {
    /// 平台标识（如 "tavily"、"doubao_search"）
    #[param(source = "query")]
    pub platform: String,
}

/// 通用 API Token 集成状态聚合响应（按 platform 过滤，Settings 通用 Token 区块唯一数据来源）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenericTokenIntegrationStatusResponse {
    /// 当前用户在该 platform 下已绑定的 token 凭证（token 永不回显，仅尾号）
    pub credentials: Vec<GenericTokenCredentialSnapshot>,
}

/// 单个通用 API Token 凭证快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericTokenCredentialSnapshot {
    /// 凭证 ID
    pub credential_id: String,
    /// 凭证名称
    pub name: String,
    /// 平台标识
    pub platform: String,
    /// API token 尾号（后 4 位，帮助区分多凭证，不构成泄露面）
    pub api_token_tail: String,
    /// 是否为该 platform 下工具身份默认凭证
    #[serde(default)]
    pub is_default: bool,
}
