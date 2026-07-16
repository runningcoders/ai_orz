//! Message Channel related API request/response DTOs - shared between backend and frontend

use crate::enums::{ChannelStatus, ChannelType};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    /// Lark App ID
    pub lark_app_id: Option<String>,
    /// Lark App Secret (only in request, not returned in response)
    pub lark_app_secret: Option<String>,
    /// Lark encryption key (only in request, not returned in response)
    pub lark_encrypt_key: Option<String>,
    /// Lark verification token (only in request, not returned in response)
    pub lark_verification_token: Option<String>,
    /// Lark user Open ID (for P2P message binding)
    pub lark_open_id: Option<String>,
    /// Lark user display name (for display, optional)
    pub lark_user_name: Option<String>,
    /// WeChat App ID
    pub wechat_app_id: Option<String>,
    /// WeChat App Secret (only in request, not returned in response)
    pub wechat_app_secret: Option<String>,
    /// WeChat Open ID
    pub wechat_open_id: Option<String>,
    /// SMTP server host
    pub email_smtp_host: Option<String>,
    /// SMTP server port
    pub email_smtp_port: Option<u16>,
    /// Email username
    pub email_username: Option<String>,
    /// Email password (only in request, not returned in response)
    pub email_password: Option<String>,
    /// From email address
    pub email_from_address: Option<String>,
    /// To email address
    pub email_to_address: Option<String>,
    /// Slack Bot Token (only in request, not returned in response)
    pub slack_bot_token: Option<String>,
    /// Slack channel ID
    pub slack_channel_id: Option<String>,
    /// Webhook HTTP method
    pub webhook_method: Option<String>,
    /// Webhook body template
    pub webhook_body_template: Option<String>,
}

/// Get Message Channel request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetMessageChannelRequest {
    /// Channel ID
    #[param(source = "path")]
    pub id: String,
}

/// Delete Message Channel request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteMessageChannelRequest {
    /// Channel ID
    #[param(source = "path")]
    pub id: String,
}

/// Message Channel list query parameters
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListMessageChannelsRequest {
    /// Filter by user ID; defaults to current logged-in user if empty
    pub user_id: Option<String>,
    /// Filter by Agent ID
    pub agent_id: Option<String>,
    /// Filter by channel type
    pub channel_type: Option<ChannelType>,
    /// Only return enabled channels
    pub only_enabled: Option<bool>,
    /// Limit result count
    pub limit: Option<usize>,
    /// Skip count
    pub offset: Option<usize>,
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
    /// Lark App ID
    pub lark_app_id: Option<String>,
    /// Lark App Secret (only in request, not returned in response)
    pub lark_app_secret: Option<String>,
    /// Lark encryption key (only in request, not returned in response)
    pub lark_encrypt_key: Option<String>,
    /// Lark verification token (only in request, not returned in response)
    pub lark_verification_token: Option<String>,
    /// Lark user Open ID (for P2P message binding)
    pub lark_open_id: Option<String>,
    /// Lark user display name (for display, optional)
    pub lark_user_name: Option<String>,
    /// WeChat App ID
    pub wechat_app_id: Option<String>,
    /// WeChat App Secret (only in request, not returned in response)
    pub wechat_app_secret: Option<String>,
    /// WeChat Open ID
    pub wechat_open_id: Option<String>,
    /// SMTP server host
    pub email_smtp_host: Option<String>,
    /// SMTP server port
    pub email_smtp_port: Option<u16>,
    /// Email username
    pub email_username: Option<String>,
    /// Email password (only in request, not returned in response)
    pub email_password: Option<String>,
    /// From email address
    pub email_from_address: Option<String>,
    /// To email address
    pub email_to_address: Option<String>,
    /// Slack Bot Token (only in request, not returned in response)
    pub slack_bot_token: Option<String>,
    /// Slack channel ID
    pub slack_channel_id: Option<String>,
    /// Webhook HTTP method
    pub webhook_method: Option<String>,
    /// Webhook body template
    pub webhook_body_template: Option<String>,
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
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
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
