//! 邮箱机器人集成（用户级）API DTO - 前后端共享
//!
//! 路由统一挂 `/api/v1/finance/identity/email/`：
//! - 邮箱机器人凭证 CRUD（user_credentials 独立表，密码/授权码加密落库永不回显）
//! - 默认凭证（同 provider 下多条凭证时渠道出站优先取默认）
//! - 集成状态聚合（platform 可选过滤，空串返回全部提供商）
//!
//! 邮箱机器人 = 用户自建代理邮箱（阶段一仅出站推送，IMAP 入站为二期）。
//! platform = 邮箱提供商标识（如 "qq"、"163"），仅作展示与默认槽位隔离维度，
//! SMTP/IMAP 连接参数以凭证 detail 为准，匹配键 `(CredentialKind::EmailBot, platform)`。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ==================== 凭证 CRUD ====================

/// 创建邮箱机器人凭证请求（手动录入自建代理邮箱的 SMTP/IMAP 信息）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateEmailBotCredentialRequest {
    /// 凭证名称（用户自命名，如「QQ 代理邮箱」）
    pub name: String,
    /// 邮箱提供商标识（如 "qq"、"163"，决定表单预设与展示样式）
    pub platform: String,
    /// 代理邮箱地址（对外主标识，如 bot@qq.com）
    pub email_address: String,
    /// SMTP 主机（如 smtp.qq.com）
    pub smtp_host: String,
    /// SMTP 端口（如 465）
    pub smtp_port: u16,
    /// IMAP 主机（如 imap.qq.com，二期入站使用）
    pub imap_host: String,
    /// IMAP 端口（如 993）
    pub imap_port: u16,
    /// 登录账号（多数提供商与邮箱地址相同）
    pub username: String,
    /// 登录密码 / 授权码（落库加密，永不回显）
    pub password: String,
}

/// 创建邮箱机器人凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEmailBotCredentialResponse {
    /// 凭证 ID
    pub credential_id: String,
}

/// 更新邮箱机器人凭证请求（path 参数：id）
///
/// platform 由凭证 ID 唯一确定，无需重复传入；各字段 None/空白保持不变，
/// 端口 None/0 保持不变。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateEmailBotCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
    /// 新名称（空白不变）
    #[serde(default)]
    pub name: Option<String>,
    /// 新代理邮箱地址（空白不变）
    #[serde(default)]
    pub email_address: Option<String>,
    /// 新 SMTP 主机（空白不变）
    #[serde(default)]
    pub smtp_host: Option<String>,
    /// 新 SMTP 端口（None/0 不变）
    #[serde(default)]
    pub smtp_port: Option<u16>,
    /// 新 IMAP 主机（空白不变）
    #[serde(default)]
    pub imap_host: Option<String>,
    /// 新 IMAP 端口（None/0 不变）
    #[serde(default)]
    pub imap_port: Option<u16>,
    /// 新登录账号（空白不变）
    #[serde(default)]
    pub username: Option<String>,
    /// 新登录密码 / 授权码（空白不变，非空重新加密）
    #[serde(default)]
    pub password: Option<String>,
}

/// 更新邮箱机器人凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEmailBotCredentialResponse {
    /// 是否更新成功
    pub success: bool,
}

/// 删除邮箱机器人凭证请求（path 参数：id）
///
/// platform 由凭证 ID 唯一确定，无需重复传入。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteEmailBotCredentialRequest {
    /// 凭证 ID
    #[param(source = "path")]
    pub id: String,
}

/// 删除邮箱机器人凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteEmailBotCredentialResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 设置默认邮箱机器人凭证请求
///
/// 默认作用域按 (kind=EmailBot, platform) 隔离，因此 platform 必填；
/// credential_id 空串表示取消该 provider 下的默认，回退取第一条。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetDefaultEmailBotCredentialRequest {
    /// 邮箱提供商标识
    pub platform: String,
    /// 凭证 ID（空串表示取消默认）
    pub credential_id: String,
}

/// 设置默认邮箱机器人凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetDefaultEmailBotCredentialResponse {
    /// 是否设置成功
    pub success: bool,
}

// ==================== 状态聚合 ====================

/// 邮箱机器人集成状态请求（query 参数：platform 可选）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct EmailIntegrationStatusRequest {
    /// 邮箱提供商标识（如 "qq"、"163"）；空串 = 不过滤，返回全部提供商
    #[param(source = "query")]
    pub platform: String,
}

/// 邮箱机器人集成状态聚合响应（按 provider 过滤，Settings 邮箱机器人区块唯一数据来源）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailIntegrationStatusResponse {
    /// 当前用户在该 provider 下已绑定的邮箱机器人凭证（密码永不回显，仅尾号）
    pub credentials: Vec<EmailBotCredentialSnapshot>,
}

/// 单个邮箱机器人凭证快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailBotCredentialSnapshot {
    /// 凭证 ID
    pub credential_id: String,
    /// 凭证名称
    pub name: String,
    /// 邮箱提供商标识
    pub platform: String,
    /// 代理邮箱地址
    pub email_address: String,
    /// SMTP 主机
    pub smtp_host: String,
    /// SMTP 端口
    pub smtp_port: u16,
    /// IMAP 主机
    pub imap_host: String,
    /// IMAP 端口
    pub imap_port: u16,
    /// 登录账号
    pub username: String,
    /// 登录密码 / 授权码尾号（后 4 位，帮助区分多凭证，不构成泄露面）
    pub password_tail: String,
    /// 是否为该 provider 下渠道出站默认凭证
    #[serde(default)]
    pub is_default: bool,
}
