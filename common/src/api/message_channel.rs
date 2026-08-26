//! Message Channel related API request/response DTOs - shared between backend and frontend

use crate::api::PaginationParams;
use crate::enums::{ChannelStatus, ChannelType};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Nested channel configuration structs (reusable across multiple DTOs)
// ---------------------------------------------------------------------------

/// Lark 渠道配置（非敏感展示字段，用于响应 DTO）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct LarkChannelConfig {
    /// 凭证引用 ID
    pub credential_id: Option<String>,
    /// 凭证名称（反查 users.identity_credentials，未找到为 None）
    pub credential_name: Option<String>,
    /// 身份模式（auto/bot/user）
    pub identity_mode: Option<String>,
    /// 用户 Open ID
    pub open_id: Option<String>,
    /// 用户昵称
    pub user_name: Option<String>,
    /// 是否监听入站消息（缺省 true）
    pub listen_inbound: bool,
}

/// 微信渠道配置（非敏感展示字段，用于响应 DTO）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct WechatChannelConfig {
    /// 微信 Open ID
    pub open_id: Option<String>,
}

/// 邮件渠道配置（非敏感展示字段，用于响应 DTO）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct EmailChannelConfig {
    /// SMTP 服务器地址
    pub smtp_host: Option<String>,
    /// SMTP 端口
    pub smtp_port: Option<u16>,
    /// 用户名
    pub username: Option<String>,
    /// 发件地址
    pub from_address: Option<String>,
    /// 收件地址
    pub to_address: Option<String>,
}

/// Slack 渠道配置（非敏感展示字段，用于响应 DTO）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SlackChannelConfig {
    /// Channel ID
    pub channel_id: Option<String>,
}

/// Webhook 渠道配置（非敏感展示字段，用于响应 DTO）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct WebhookChannelConfig {
    /// HTTP 方法
    pub method: Option<String>,
    /// 请求体模板
    pub body_template: Option<String>,
}

/// 消息渠道配置总结构体（用于响应 DTO，存放各渠道类型的非敏感展示字段）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct MessageChannelConfig {
    /// 飞书渠道配置
    pub lark: Option<LarkChannelConfig>,
    /// 微信渠道配置
    pub wechat: Option<WechatChannelConfig>,
    /// 邮件渠道配置
    pub email: Option<EmailChannelConfig>,
    /// Slack 渠道配置
    pub slack: Option<SlackChannelConfig>,
    /// Webhook 渠道配置
    pub webhook: Option<WebhookChannelConfig>,
}

// ---------------------------------------------------------------------------
// Request-side nested config structs (include sensitive fields)
// ---------------------------------------------------------------------------

/// 创建/更新请求 - Lark 配置（含敏感字段）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct CreateLarkChannelConfig {
    /// 凭证引用 ID
    pub credential_id: Option<String>,
    /// 身份模式（auto/bot/user）
    pub identity_mode: Option<String>,
    /// 用户 Open ID
    pub open_id: Option<String>,
    /// 用户昵称
    pub user_name: Option<String>,
    /// 是否监听入站消息
    pub listen_inbound: Option<bool>,
}

/// 创建/更新请求 - 微信配置（含敏感字段）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct CreateWechatChannelConfig {
    /// App ID
    pub app_id: Option<String>,
    /// App Secret（敏感字段）
    pub app_secret: Option<String>,
    /// 用户 Open ID
    pub open_id: Option<String>,
}

/// 创建/更新请求 - 邮件配置（含敏感字段）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct CreateEmailChannelConfig {
    /// SMTP 服务器地址
    pub smtp_host: Option<String>,
    /// SMTP 端口
    pub smtp_port: Option<u16>,
    /// 用户名
    pub username: Option<String>,
    /// 密码（敏感字段）
    pub password: Option<String>,
    /// 发件地址
    pub from_address: Option<String>,
    /// 收件地址
    pub to_address: Option<String>,
}

/// 创建/更新请求 - Slack 配置（含敏感字段）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct CreateSlackChannelConfig {
    /// Bot Token（敏感字段）
    pub bot_token: Option<String>,
    /// Channel ID
    pub channel_id: Option<String>,
}

/// 创建/更新请求 - Webhook 配置
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct CreateWebhookChannelConfig {
    /// HTTP 方法
    pub method: Option<String>,
    /// 请求体模板
    pub body_template: Option<String>,
}

/// 创建/更新请求 - 渠道配置总结构体（用于请求 DTO，含各渠道敏感字段）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct CreateMessageChannelConfig {
    /// 飞书渠道配置
    pub lark: Option<CreateLarkChannelConfig>,
    /// 微信渠道配置
    pub wechat: Option<CreateWechatChannelConfig>,
    /// 邮件渠道配置
    pub email: Option<CreateEmailChannelConfig>,
    /// Slack 渠道配置
    pub slack: Option<CreateSlackChannelConfig>,
    /// Webhook 渠道配置
    pub webhook: Option<CreateWebhookChannelConfig>,
}

/// Create Message Channel request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateMessageChannelRequest {
    /// Bound user ID; defaults to current logged-in user if empty
    pub user_id: Option<String>,
    /// Associated Agent ID; None means user global default channel
    pub agent_id: Option<String>,
    /// Channel type
    pub channel_type: ChannelType,
    /// User custom channel name
    pub channel_name: String,
    /// Webhook URL (for Feishu, Slack, generic webhook)
    pub webhook_url: Option<String>,
    /// Access token (only in request, not returned in response)
    pub access_token: Option<String>,
    /// Signing secret (only in request, not returned in response)
    pub secret: Option<String>,
    /// Channel-specific configuration (lark/wechat/email/slack/webhook)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<CreateMessageChannelConfig>,
}

/// Get Message Channel request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetMessageChannelRequest {
    /// Channel ID
    #[param(source = "path")]
    pub id: String,
}

/// Delete Message Channel request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteMessageChannelRequest {
    /// Channel ID
    #[param(source = "path")]
    pub id: String,
}

/// Message Channel list query parameters
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListMessageChannelsRequest {
    /// Filter by user ID; defaults to current logged-in user if empty
    #[param(source = "query")]
    pub user_id: Option<String>,
    /// Filter by Agent ID
    #[param(source = "query")]
    pub agent_id: Option<String>,
    /// Filter by channel type
    #[param(source = "query")]
    pub channel_type: Option<ChannelType>,
    /// Only return enabled channels
    #[param(source = "query")]
    pub only_enabled: Option<bool>,
    /// Limit result count
    #[param(source = "query")]
    pub limit: Option<usize>,
    /// Skip count
    #[param(source = "query")]
    pub offset: Option<usize>,
}

/// Message Channel 通用查询请求（POST body）
///
/// 支持完整查询条件 + 分页，query 是核心查询能力。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct MessageChannelQueryRequest {
    /// 按渠道 ID 查询（通常返回单条）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 按用户 ID 查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// 按 Agent ID 查询（用于 Agent 专属渠道）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// 按渠道类型查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_type: Option<ChannelType>,
    /// 只查询启用的渠道
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only_enabled: Option<bool>,
    /// 按状态 IN 查询（支持多选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_in: Option<Vec<ChannelStatus>>,
    /// 排序规则，如 "created_at ASC", "created_at DESC"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<String>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// List Message Channels response
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListMessageChannelsResponse {
    /// List of message channels
    pub channels: Vec<MessageChannelListItem>,
    /// Total count matching query
    pub total: usize,
}

/// Message Channel list item alias (frontend compatibility)
pub type ListMessageChannelsResponseItem = MessageChannelListItem;

/// Update Message Channel request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateMessageChannelRequest {
    /// Channel ID
    #[param(source = "path")]
    pub id: String,

    /// Bound user ID
    pub user_id: Option<String>,
    /// Associated Agent ID; None means no change
    pub agent_id: Option<String>,
    /// Channel type
    pub channel_type: Option<ChannelType>,
    /// User custom channel name
    pub channel_name: Option<String>,
    /// Webhook URL
    pub webhook_url: Option<String>,
    /// Access token (only in request, not returned in response)
    pub access_token: Option<String>,
    /// Signing secret (only in request, not returned in response)
    pub secret: Option<String>,
    /// Channel-specific configuration (lark/wechat/email/slack/webhook)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<CreateMessageChannelConfig>,
}

/// Update Message Channel status request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateMessageChannelStatusRequest {
    /// Channel ID
    #[param(source = "path")]
    pub id: String,
    /// Target status. Deleted is not allowed via this API, use DELETE instead.
    pub status: ChannelStatus,
}

/// Test Message Channel connection request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct TestMessageChannelConnectionRequest {
    /// Channel ID
    #[param(source = "path")]
    pub id: String,
}

/// Test Message Channel connection response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TestMessageChannelConnectionResponse {
    /// Whether connection test succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Create Message Channel response
pub type CreateMessageChannelResponse = MessageChannelDetail;

/// Update Message Channel response
pub type UpdateMessageChannelResponse = MessageChannelDetail;

/// Update Message Channel status response
pub type UpdateMessageChannelStatusResponse = MessageChannelDetail;

/// Get Message Channel response
pub type GetMessageChannelResponse = MessageChannelDetail;

/// Message Channel list item response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MessageChannelListItem {
    /// Channel ID
    pub id: String,
    /// Organization ID
    pub org_id: String,
    /// Bound user ID
    pub user_id: String,
    /// Associated Agent ID
    pub agent_id: Option<String>,
    /// Channel type
    pub channel_type: ChannelType,
    /// User custom channel name
    pub channel_name: String,
    /// Webhook URL
    pub webhook_url: Option<String>,
    /// Channel status
    pub status: ChannelStatus,
    /// Whether access token is configured
    pub has_access_token: bool,
    /// Whether secret is configured
    pub has_secret: bool,
    /// Whether there are sensitive fields in config
    pub has_config_secret: bool,
    /// Channel-specific configuration (non-sensitive display fields)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<MessageChannelConfig>,
    /// Last successful push timestamp
    pub last_pushed_at: Option<i64>,
    /// Last push error message
    pub last_error: Option<String>,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
}

/// Message Channel detail response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MessageChannelDetail {
    /// Channel ID
    pub id: String,
    /// Organization ID
    pub org_id: String,
    /// Bound user ID
    pub user_id: String,
    /// Associated Agent ID
    pub agent_id: Option<String>,
    /// Channel type
    pub channel_type: ChannelType,
    /// User custom channel name
    pub channel_name: String,
    /// Webhook URL
    pub webhook_url: Option<String>,
    /// Channel status
    pub status: ChannelStatus,
    /// Whether access token is configured
    pub has_access_token: bool,
    /// Whether secret is configured
    pub has_secret: bool,
    /// Whether there are sensitive fields in config
    pub has_config_secret: bool,
    /// Channel-specific configuration (non-sensitive display fields)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<MessageChannelConfig>,
    /// Last successful push timestamp
    pub last_pushed_at: Option<i64>,
    /// Last push error message
    pub last_error: Option<String>,
    /// Creator ID
    pub created_by: String,
    /// Last modifier ID
    pub modified_by: String,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
}
