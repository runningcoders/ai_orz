//! GitHub 集成（用户级）API DTO - 前后端共享
//!
//! 路由统一挂 `/api/v1/finance/identity/github/`：
//! - PAT 凭证 CRUD（users 表 identity_credentials JSON 列，token 加密落库永不回显）
//! - 登录态探测（gh auth status --json 实测，凭证绑定后由 gh_cli 工具自动登录）
//! - 默认凭证（多条 token 时 gh_cli 工具身份优先取默认）

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ==================== 凭证 CRUD ====================

/// 创建 GitHub 凭证请求（手动录入 PAT）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateGithubCredentialRequest {
    /// 凭证名称（用户自命名，如「工作号」「个人号」）
    pub name: String,
    /// Personal Access Token（落库加密，永不回显）
    pub token: String,
}

/// 创建 GitHub 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGithubCredentialResponse {
    /// 凭证 ID
    pub credential_id: String,
}

/// 更新 GitHub 凭证请求（path 参数：credential_id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateGithubCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
    /// 新名称（空白不变）
    #[serde(default)]
    pub name: Option<String>,
    /// 新 Token（空白不变，非空重新加密；变更后 gh_cli 自动重新登录）
    #[serde(default)]
    pub token: Option<String>,
}

/// 更新 GitHub 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateGithubCredentialResponse {
    /// 是否更新成功
    pub success: bool,
}

/// 删除 GitHub 凭证请求（path 参数：credential_id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteGithubCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
}

/// 删除 GitHub 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteGithubCredentialResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 设置默认 GitHub 凭证请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetDefaultGithubCredentialRequest {
    /// 凭证 ID（空串表示取消默认，回退取第一条）
    pub credential_id: String,
}

/// 设置默认 GitHub 凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetDefaultGithubCredentialResponse {
    /// 是否设置成功
    pub success: bool,
}

// ==================== 状态聚合 ====================

/// GitHub 集成状态请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GithubIntegrationStatusRequest {}

/// GitHub 集成状态聚合响应（Settings GitHub 区块唯一数据来源）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubIntegrationStatusResponse {
    /// 当前用户已绑定的 GitHub 凭证（token 永不回显，仅尾号）
    pub credentials: Vec<GithubCredentialSnapshot>,
    /// gh 登录态实测（HOME 下 gh auth status）
    pub auth: GithubAuthSnapshot,
}

/// 单个 GitHub 凭证快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubCredentialSnapshot {
    /// 凭证 ID
    pub credential_id: String,
    /// 凭证名称
    pub name: String,
    /// token 尾号（后 4 位，帮助区分多凭证，不构成泄露面）
    pub token_tail: String,
    /// 是否为 gh_cli 工具身份默认凭证
    #[serde(default)]
    pub is_default: bool,
}

/// gh 登录态快照
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubAuthSnapshot {
    /// HOME 下 gh 是否已登录
    pub logged_in: bool,
    /// 已登录 GitHub 账号名
    #[serde(default)]
    pub user_name: Option<String>,
    /// 引导提示（如 gh 未安装）
    #[serde(default)]
    pub hint: Option<String>,
}
