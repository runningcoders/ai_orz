//! Message channel related enums

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;
use std::fmt;

/// Channel type (推送渠道类型)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ChannelType {
    /// 飞书
    #[default]
    Lark = 0,
    /// 微信
    Wechat = 1,
    /// Slack
    Slack = 2,
    /// 邮件
    Email = 3,
    /// 通用 Webhook
    Webhook = 4,
    /// A2A 协议回调
    A2aCallback = 5,
}

impl From<i32> for ChannelType {
    fn from(v: i32) -> Self {
        match v {
            0 => ChannelType::Lark,
            1 => ChannelType::Wechat,
            2 => ChannelType::Slack,
            3 => ChannelType::Email,
            4 => ChannelType::Webhook,
            5 => ChannelType::A2aCallback,
            _ => ChannelType::default(),
        }
    }
}

impl ChannelType {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<ChannelType> for i32 {
    fn from(r: ChannelType) -> i32 {
        r as i32
    }
}

impl ChannelType {
    /// Get channel type name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelType::Lark => "lark",
            ChannelType::Wechat => "wechat",
            ChannelType::Slack => "slack",
            ChannelType::Email => "email",
            ChannelType::Webhook => "webhook",
            ChannelType::A2aCallback => "a2a_callback",
        }
    }
}

impl fmt::Display for ChannelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 渠道状态
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ChannelStatus {
    /// 已删除（软删除）
    Deleted = 0,
    /// 活跃
    Active = 1,
    /// 已禁用
    Disabled = 2,
}

impl Default for ChannelStatus {
    fn default() -> Self {
        ChannelStatus::Active
    }
}

impl From<i32> for ChannelStatus {
    fn from(v: i32) -> Self {
        match v {
            1 => ChannelStatus::Active,
            2 => ChannelStatus::Disabled,
            _ => ChannelStatus::Deleted,
        }
    }
}

impl From<ChannelStatus> for i32 {
    fn from(r: ChannelStatus) -> i32 {
        r as i32
    }
}

impl fmt::Display for ChannelStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelStatus::Deleted => write!(f, "deleted"),
            ChannelStatus::Active => write!(f, "active"),
            ChannelStatus::Disabled => write!(f, "disabled"),
        }
    }
}
