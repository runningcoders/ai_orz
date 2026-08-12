//! 飞书集成（用户级）API DTO - 前后端共享
//!
//! 路由统一挂 `/api/v1/finance/identity/lark/`：
//! - 凭证 CRUD（users 表 identity_credentials JSON 列）
//! - 用户 OAuth device flow（auth start/complete/status/logout）
//! - 绑定快照聚合（status）
//! - config init --new 自动化绑定（bind start/status/cancel）

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ==================== 凭证 CRUD ====================

/// 创建飞书应用凭证请求（手动录入）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateLarkCredentialRequest {
    /// 凭证名称（用户自命名）
    pub name: String,
    /// 飞书 App ID
    pub app_id: String,
    /// 飞书 App Secret（落库加密，永不回显）
    pub app_secret: String,
    /// Encrypt Key（可选，事件校验用）
    #[serde(default)]
    pub encrypt_key: Option<String>,
    /// Verification Token（可选）
    #[serde(default)]
    pub verification_token: Option<String>,
}

/// 创建飞书应用凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLarkCredentialResponse {
    /// 凭证 ID（渠道引用键）
    pub credential_id: String,
}

/// 更新飞书应用凭证请求（path 参数：credential_id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateLarkCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
    /// 新名称（空白不变）
    #[serde(default)]
    pub name: Option<String>,
    /// 新 App ID（空白不变；变化触发渠道重建联）
    #[serde(default)]
    pub app_id: Option<String>,
    /// 新 App Secret（空白不变，非空重新加密）
    #[serde(default)]
    pub app_secret: Option<String>,
    /// 新 Encrypt Key（空白不变）
    #[serde(default)]
    pub encrypt_key: Option<String>,
    /// 新 Verification Token（空白视为清除）
    #[serde(default)]
    pub verification_token: Option<String>,
}

/// 更新飞书应用凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateLarkCredentialResponse {
    /// 是否更新成功
    pub success: bool,
}

/// 删除飞书应用凭证请求（path 参数：credential_id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteLarkCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
}

/// 删除飞书应用凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteLarkCredentialResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 设置默认凭证请求（lark_cli 工具身份优先取引用该凭证的渠道）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetDefaultLarkCredentialRequest {
    /// 凭证 ID（空串表示取消默认）
    pub credential_id: String,
}

/// 设置默认凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetDefaultLarkCredentialResponse {
    /// 是否设置成功
    pub success: bool,
}

// ==================== 用户 OAuth device flow ====================

/// 发起 device flow 授权请求
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LarkAuthStartRequest {
    /// 业务域列表（空则请求全部域）
    #[serde(default)]
    pub domains: Vec<String>,
}

/// 发起 device flow 授权响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkAuthStartResponse {
    /// 设备码（complete 时回传）
    pub device_code: String,
    /// 用户浏览器验证 URL
    pub verification_url: String,
    /// 设备码有效期（秒）
    #[serde(default)]
    pub expires_in: Option<u64>,
}

/// 完成 device flow 授权请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LarkAuthCompleteRequest {
    /// start 返回的设备码
    pub device_code: String,
}

/// 完成 device flow 授权响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkAuthCompleteResponse {
    /// 是否授权成功
    pub success: bool,
    /// keychain 等环境降级标记
    #[serde(default)]
    pub degraded: bool,
    /// 降级/失败提示
    #[serde(default)]
    pub hint: Option<String>,
}

/// 用户授权状态请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct LarkAuthStatusRequest {}

/// 用户授权状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkAuthStatusResponse {
    /// 用户身份是否已授权
    pub logged_in: bool,
    /// 已授权用户名
    #[serde(default)]
    pub user_name: Option<String>,
    /// 环境降级标记（keychain 不可用等）
    #[serde(default)]
    pub degraded: bool,
    /// 降级/引导提示
    #[serde(default)]
    pub hint: Option<String>,
}

/// 取消用户授权请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct LarkAuthLogoutRequest {}

/// 取消用户授权响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkAuthLogoutResponse {
    /// 是否取消成功
    pub success: bool,
    /// 环境降级标记
    #[serde(default)]
    pub degraded: bool,
    /// 降级/失败提示
    #[serde(default)]
    pub hint: Option<String>,
}

// ==================== 绑定快照聚合 ====================

/// 绑定快照请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct LarkIntegrationStatusRequest {}

/// 绑定快照聚合响应（Settings 飞书集成区块唯一数据来源）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LarkIntegrationStatusResponse {
    /// 当前用户已绑定的凭证（含引用渠道明细）
    pub credentials: Vec<LarkCredentialSnapshot>,
    /// 用户 OAuth 授权现状（现场执行 auth status --json）
    pub user_auth: LarkUserAuthSnapshot,
}

/// 单个凭证的绑定快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkCredentialSnapshot {
    /// 凭证 ID
    pub credential_id: String,
    /// 凭证名称
    pub name: String,
    /// 飞书 App ID
    pub app_id: String,
    /// 是否为用户选定的默认凭证（lark_cli 工具身份优先）
    #[serde(default)]
    pub is_default: bool,
    /// 引用该凭证的渠道明细
    #[serde(default)]
    pub channels: Vec<LarkCredentialChannelRef>,
}

/// 凭证关联渠道引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkCredentialChannelRef {
    /// 渠道 ID
    pub channel_id: String,
    /// 渠道名称
    pub channel_name: String,
    /// 渠道启用状态
    pub enabled: bool,
}

/// 用户授权快照
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LarkUserAuthSnapshot {
    /// 用户身份是否已授权
    pub logged_in: bool,
    /// 已授权用户名
    #[serde(default)]
    pub user_name: Option<String>,
    /// 环境降级标记
    #[serde(default)]
    pub degraded: bool,
    /// 降级/引导提示
    #[serde(default)]
    pub hint: Option<String>,
}

// ==================== config init --new 自动化绑定 ====================

/// 发起自动绑定请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct LarkBindStartRequest {}

/// 发起自动绑定响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkBindStartResponse {
    /// 绑定会话 ID
    pub session_id: String,
    /// 飞书验证 URL（用户在浏览器完成建应用授权）
    pub verification_url: String,
}

/// 绑定会话状态查询请求（query 参数：session_id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct LarkBindStatusRequest {
    /// 绑定会话 ID
    #[param(source = "query")]
    pub session_id: String,
}

/// 绑定会话状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkBindStatusResponse {
    /// pending / done / failed
    pub status: String,
    /// done 且 secret 可读：已写入的凭证 ID（分支 A）
    #[serde(default)]
    pub credential_id: Option<String>,
    /// done 且联动创建的首条渠道 ID（分支 A）
    #[serde(default)]
    pub channel_id: Option<String>,
    /// done 但 secret 不可读：需补填（分支 B，app_id 预填）
    #[serde(default)]
    pub app_id: Option<String>,
    /// 验证 URL（启动窗口未抓到时前端轮询补取）
    #[serde(default)]
    pub verification_url: Option<String>,
    /// failed 时的错误提示（脱敏）
    #[serde(default)]
    pub error: Option<String>,
}

/// 取消绑定会话请求
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LarkBindCancelRequest {
    /// 绑定会话 ID
    pub session_id: String,
}

/// 取消绑定会话响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkBindCancelResponse {
    /// 是否取消成功
    pub success: bool,
}
