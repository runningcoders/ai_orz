//! Message Channel related API request/response DTOs - shared between backend and frontend

use crate::enums::{ChannelStatus, ChannelType};
use serde::{Deserialize, Serialize};

/// 创建 Message Channel 请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateMessageChannelRequest {
    /// 绑定用户 ID；为空时默认使用当前登录用户
    pub user_id: Option<String>,
    /// 关联 Agent ID；None 表示用户全局默认渠道
    pub agent_id: Option<String>,
    /// 渠道类型
    pub channel_type: ChannelType,
    /// 用户自定义渠道名称
    pub channel_name: String,
    /// Webhook 地址（飞书、Slack、通用 Webhook 等使用）
    pub webhook_url: Option<String>,
    /// 访问 Token（仅请求入参，不会在响应中返回）
    pub access_token: Option<String>,
    /// 签名密钥/Secret（仅请求入参，不会在响应中返回）
    pub secret: Option<String>,
    /// Lark App ID
    pub lark_app_id: Option<String>,
    /// Lark App Secret（仅请求入参，不会在响应中返回）
    pub lark_app_secret: Option<String>,
    /// Lark 加密密钥（仅请求入参，不会在响应中返回）
    pub lark_encrypt_key: Option<String>,
    /// Lark 验证令牌（仅请求入参，不会在响应中返回）
    pub lark_verification_token: Option<String>,
    /// WeChat App ID
    pub wechat_app_id: Option<String>,
    /// WeChat App Secret（仅请求入参，不会在响应中返回）
    pub wechat_app_secret: Option<String>,
    /// WeChat Open ID
    pub wechat_open_id: Option<String>,
    /// SMTP 服务器地址
    pub email_smtp_host: Option<String>,
    /// SMTP 服务器端口
    pub email_smtp_port: Option<u16>,
    /// 邮箱用户名
    pub email_username: Option<String>,
    /// 邮箱密码（仅请求入参，不会在响应中返回）
    pub email_password: Option<String>,
    /// 发件人邮箱
    pub email_from_address: Option<String>,
    /// 收件人邮箱
    pub email_to_address: Option<String>,
    /// Slack Bot Token（仅请求入参，不会在响应中返回）
    pub slack_bot_token: Option<String>,
    /// Slack 频道 ID
    pub slack_channel_id: Option<String>,
    /// Webhook HTTP 方法
    pub webhook_method: Option<String>,
    /// Webhook 请求体模板
    pub webhook_body_template: Option<String>,
}

/// Message Channel 列表查询参数
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MessageChannelListQuery {
    /// 按用户 ID 查询；为空时默认当前登录用户
    pub user_id: Option<String>,
    /// 按 Agent ID 查询
    pub agent_id: Option<String>,
    /// 按渠道类型查询
    pub channel_type: Option<ChannelType>,
    /// 只查询启用渠道
    pub only_enabled: Option<bool>,
    /// 限制返回条数
    pub limit: Option<usize>,
    /// 跳过条数
    pub offset: Option<usize>,
}

/// 更新 Message Channel 请求
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct UpdateMessageChannelRequest {
    /// 绑定用户 ID
    pub user_id: Option<String>,
    /// 关联 Agent ID；None 表示不修改
    pub agent_id: Option<String>,
    /// 渠道类型
    pub channel_type: Option<ChannelType>,
    /// 用户自定义渠道名称
    pub channel_name: Option<String>,
    /// Webhook 地址
    pub webhook_url: Option<String>,
    /// 访问 Token（仅请求入参，不会在响应中返回）
    pub access_token: Option<String>,
    /// 签名密钥/Secret（仅请求入参，不会在响应中返回）
    pub secret: Option<String>,
    /// Lark App ID
    pub lark_app_id: Option<String>,
    /// Lark App Secret（仅请求入参，不会在响应中返回）
    pub lark_app_secret: Option<String>,
    /// Lark 加密密钥（仅请求入参，不会在响应中返回）
    pub lark_encrypt_key: Option<String>,
    /// Lark 验证令牌（仅请求入参，不会在响应中返回）
    pub lark_verification_token: Option<String>,
    /// WeChat App ID
    pub wechat_app_id: Option<String>,
    /// WeChat App Secret（仅请求入参，不会在响应中返回）
    pub wechat_app_secret: Option<String>,
    /// WeChat Open ID
    pub wechat_open_id: Option<String>,
    /// SMTP 服务器地址
    pub email_smtp_host: Option<String>,
    /// SMTP 服务器端口
    pub email_smtp_port: Option<u16>,
    /// 邮箱用户名
    pub email_username: Option<String>,
    /// 邮箱密码（仅请求入参，不会在响应中返回）
    pub email_password: Option<String>,
    /// 发件人邮箱
    pub email_from_address: Option<String>,
    /// 收件人邮箱
    pub email_to_address: Option<String>,
    /// Slack Bot Token（仅请求入参，不会在响应中返回）
    pub slack_bot_token: Option<String>,
    /// Slack 频道 ID
    pub slack_channel_id: Option<String>,
    /// Webhook HTTP 方法
    pub webhook_method: Option<String>,
    /// Webhook 请求体模板
    pub webhook_body_template: Option<String>,
}

/// 更新 Message Channel 状态请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateMessageChannelStatusRequest {
    /// 目标状态。Deleted 不允许通过状态更新接口产生，请使用 DELETE 接口。
    pub status: ChannelStatus,
}

/// 测试 Message Channel 连通性响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMessageChannelConnectionResponse {
    /// 连通性测试是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
}

/// 创建 Message Channel 响应
pub type CreateMessageChannelResponse = MessageChannelDetail;

/// 更新 Message Channel 响应
pub type UpdateMessageChannelResponse = MessageChannelDetail;

/// 更新 Message Channel 状态响应
pub type UpdateMessageChannelStatusResponse = MessageChannelDetail;

/// 获取 Message Channel 响应
pub type GetMessageChannelResponse = MessageChannelDetail;

/// Message Channel 列表项响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageChannelListItem {
    /// 渠道 ID
    pub id: String,
    /// 组织 ID
    pub org_id: String,
    /// 绑定用户 ID
    pub user_id: String,
    /// 关联 Agent ID
    pub agent_id: Option<String>,
    /// 渠道类型
    pub channel_type: ChannelType,
    /// 用户自定义渠道名称
    pub channel_name: String,
    /// Webhook 地址
    pub webhook_url: Option<String>,
    /// 渠道状态
    pub status: ChannelStatus,
    /// 是否配置 access token
    pub has_access_token: bool,
    /// 是否配置 secret
    pub has_secret: bool,
    /// 是否存在配置中的敏感字段
    pub has_config_secret: bool,
    /// 最后成功推送时间戳
    pub last_pushed_at: Option<i64>,
    /// 最后一次推送错误信息
    pub last_error: Option<String>,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// Message Channel 详情响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageChannelDetail {
    /// 渠道 ID
    pub id: String,
    /// 组织 ID
    pub org_id: String,
    /// 绑定用户 ID
    pub user_id: String,
    /// 关联 Agent ID
    pub agent_id: Option<String>,
    /// 渠道类型
    pub channel_type: ChannelType,
    /// 用户自定义渠道名称
    pub channel_name: String,
    /// Webhook 地址
    pub webhook_url: Option<String>,
    /// 渠道状态
    pub status: ChannelStatus,
    /// 是否配置 access token
    pub has_access_token: bool,
    /// 是否配置 secret
    pub has_secret: bool,
    /// 是否存在配置中的敏感字段
    pub has_config_secret: bool,
    /// 最后成功推送时间戳
    pub last_pushed_at: Option<i64>,
    /// 最后一次推送错误信息
    pub last_error: Option<String>,
    /// 创建人 ID
    pub created_by: String,
    /// 最后修改人 ID
    pub modified_by: String,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}
