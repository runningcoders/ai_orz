//! MessageChannel 实体
//!
//! 对应 SQL 建表语句：`migrations/20260508000000_message_channels.sql`
//!
//! 消息渠道配置：
//! - 支持为用户绑定多个推送渠道
//! - 支持为特定 Agent 绑定专用渠道
//! - 各渠道配置统一存储在 config_json 字段中（JSON 格式）

use common::constants::utils;
use common::enums::{ChannelType, ChannelStatus};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use std::collections::HashMap;

/// MessageChannel 业务实体
#[derive(Debug, Clone)]
pub struct MessageChannel {
    /// 底层持久化对象
    pub po: MessageChannelPo,
}

impl MessageChannel {
    /// 从 Po 创建 MessageChannel
    pub fn from_po(po: MessageChannelPo) -> Self {
        Self { po }
    }

    /// 转换为 Po
    pub fn into_po(self) -> MessageChannelPo {
        self.po
    }

    /// 获取渠道 ID
    pub fn id(&self) -> &str {
        self.po.id.as_str()
    }

    /// 获取用户 ID
    pub fn user_id(&self) -> &str {
        self.po.user_id.as_str()
    }

    /// 获取 Agent ID（如果有）
    pub fn agent_id(&self) -> Option<&str> {
        self.po.agent_id.as_deref()
    }

    /// 获取渠道类型
    pub fn channel_type(&self) -> ChannelType {
        self.po.channel_type
    }

    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        matches!(self.po.status, ChannelStatus::Active)
    }

    /// 获取配置
    pub fn config(&self) -> &ChannelConfig {
        &self.po.config_json.0
    }
}

/// MessageChannelPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Default, derive_builder::Builder)]
#[builder(setter(into), default)]
pub struct MessageChannelPo {
    /// 渠道 ID
    pub id: String,
    /// 组织 ID（多租户隔离）
    pub org_id: String,
    /// 绑定的用户 ID
    pub user_id: String,
    /// 关联的 Agent ID（NULL 表示用户全局默认渠道，不绑定特定 Agent）
    pub agent_id: Option<String>,
    /// 渠道类型
    pub channel_type: ChannelType,
    /// 用户自定义的渠道名称（便于区分多个同类型渠道）
    pub channel_name: String,
    /// Webhook 地址（飞书、Slack、通用 Webhook 等使用）
    pub webhook_url: Option<String>,
    /// 访问 Token（需要鉴权的渠道）
    pub access_token: Option<String>,
    /// 签名密钥/Secret
    pub secret: Option<String>,
    /// 扩展配置 JSON（各渠道的详细配置）
    pub config_json: Json<ChannelConfig>,
    /// 渠道状态
    pub status: ChannelStatus,
    /// 最后成功推送的时间戳（毫秒）
    pub last_pushed_at: Option<i64>,
    /// 最后一次推送的错误信息
    pub last_error: Option<String>,
    /// 创建人 ID
    pub created_by: String,
    /// 最后修改人 ID
    pub modified_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

impl MessageChannelPo {
    /// 创建新的 MessageChannelPo
    pub fn new(
        id: String,
        org_id: String,
        user_id: String,
        agent_id: Option<String>,
        channel_type: ChannelType,
        channel_name: String,
        webhook_url: Option<String>,
        access_token: Option<String>,
        secret: Option<String>,
        config: ChannelConfig,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp_ms();
        Self {
            id,
            org_id,
            user_id,
            agent_id,
            channel_type,
            channel_name,
            webhook_url,
            access_token,
            secret,
            config_json: Json(config),
            status: ChannelStatus::Active,
            last_pushed_at: None,
            last_error: None,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }
}

/// 渠道配置结构体（对应 config_json 字段）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelConfig {
    // 飞书配置
    /// 飞书 App ID
    pub lark_app_id: Option<String>,
    /// 飞书 App Secret
    pub lark_app_secret: Option<String>,
    /// 飞书加密密钥
    pub lark_encrypt_key: Option<String>,
    /// 飞书验证令牌
    pub lark_verification_token: Option<String>,

    // 微信配置
    /// 微信 App ID
    pub wechat_app_id: Option<String>,
    /// 微信 App Secret
    pub wechat_app_secret: Option<String>,
    /// 微信 Open ID
    pub wechat_open_id: Option<String>,

    // 邮件配置
    /// SMTP 服务器地址
    pub email_smtp_host: Option<String>,
    /// SMTP 服务器端口
    pub email_smtp_port: Option<u16>,
    /// 邮箱用户名
    pub email_username: Option<String>,
    /// 邮箱密码
    pub email_password: Option<String>,
    /// 发件人邮箱
    pub email_from_address: Option<String>,
    /// 收件人邮箱
    pub email_to_address: Option<String>,

    // Slack 配置
    /// Slack Bot Token
    pub slack_bot_token: Option<String>,
    /// Slack 频道 ID
    pub slack_channel_id: Option<String>,

    // 通用 Webhook 配置
    /// HTTP 方法（GET / POST / PUT 等）
    pub webhook_method: Option<String>,
    /// HTTP 请求头
    pub webhook_headers: Option<HashMap<String, String>>,
    /// 请求体模板（支持占位符替换）
    pub webhook_body_template: Option<String>,

    // 其他扩展字段
    pub extra: Option<HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_channel_po_builder() {
        // 验证 Builder 模式可以正常工作
        let po = MessageChannelPoBuilder::default()
            .id("channel_001".to_string())
            .org_id("org_001".to_string())
            .user_id("user_001".to_string())
            .agent_id(Some("agent_001".to_string()))
            .channel_type(ChannelType::Lark)
            .channel_name("飞书通知".to_string())
            .webhook_url(Some("https://example.com/webhook".to_string()))
            .access_token(None)
            .secret(Some("secret_123".to_string()))
            .config_json(Json(ChannelConfig::default()))
            .status(ChannelStatus::Active)
            .created_by("tester".to_string())
            .modified_by("tester".to_string())
            .created_at(1234567890)
            .updated_at(1234567890)
            .build()
            .unwrap();

        assert_eq!(po.id, "channel_001");
        assert_eq!(po.channel_name, "飞书通知");
        assert_eq!(po.agent_id, Some("agent_001".to_string()));
        assert_eq!(po.status, ChannelStatus::Active);
    }
}
