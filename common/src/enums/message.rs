//! Message related enums

use serde::{Serialize, Deserialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Message role (谁发送的消息)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum MessageRole {
    /// User (用户发送)
    #[default]
    User = 0,
    /// Agent (AI 代理发送)
    Agent = 1,
    /// System (系统消息)
    System = 2,
}

impl From<i32> for MessageRole {
    fn from(v: i32) -> Self {
        match v {
            0 => MessageRole::User,
            1 => MessageRole::Agent,
            2 => MessageRole::System,
            _ => MessageRole::default(),
        }
    }
}

impl MessageRole {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<MessageRole> for i32 {
    fn from(r: MessageRole) -> i32 {
        r as i32
    }
}

impl From<i64> for MessageRole {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// Message type (消息类型)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum MessageType {
    /// Text (纯文本)
    #[default]
    Text = 0,
    /// Image (图片)
    Image = 1,
    /// File (文件)
    File = 2,
    /// Audio (音频)
    Audio = 3,
    /// Video (视频)
    Video = 4,
    /// ToolCallRequest (工具调用请求，自建链路)
    ToolCallRequest = 5,
    /// ToolCallResult (工具调用结果，自建链路)
    ToolCallResult = 6,
}

impl From<i32> for MessageType {
    fn from(v: i32) -> Self {
        match v {
            0 => MessageType::Text,
            1 => MessageType::Image,
            2 => MessageType::File,
            3 => MessageType::Audio,
            4 => MessageType::Video,
            5 => MessageType::ToolCallRequest,
            6 => MessageType::ToolCallResult,
            _ => MessageType::default(),
        }
    }
}

impl MessageType {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<MessageType> for i32 {
    fn from(t: MessageType) -> i32 {
        t as i32
    }
}

impl From<i64> for MessageType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// Message status (处理状态，用于事件总线恢复)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum MessageStatus {
    /// Recalled (已撤回，逻辑删除，不再显示和处理)
    Recalled = 0,
    /// Pending (待处理)
    #[default]
    Pending = 1,
    /// Processing (处理中)
    Processing = 2,
    /// Processed (处理完成)
    Processed = 3,
    /// Failed (处理失败)
    Failed = 4,
}

impl From<i32> for MessageStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => MessageStatus::Recalled,
            1 => MessageStatus::Pending,
            2 => MessageStatus::Processing,
            3 => MessageStatus::Processed,
            4 => MessageStatus::Failed,
            _ => MessageStatus::default(),
        }
    }
}

impl MessageStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<MessageStatus> for i32 {
    fn from(s: MessageStatus) -> i32 {
        s as i32
    }
}

impl From<i64> for MessageStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// Control mode (工具控制模式：rig自动处理 / 自建链路手动处理)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ControlMode {
    /// Auto (rig 原生自动处理，适合简单工具)
    #[default]
    Auto = 0,
    /// Manual (自建链路处理，需要收敛控制的关键工具)
    Manual = 1,
}

impl From<i32> for ControlMode {
    fn from(v: i32) -> Self {
        match v {
            0 => ControlMode::Auto,
            1 => ControlMode::Manual,
            _ => ControlMode::default(),
        }
    }
}

impl ControlMode {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<ControlMode> for i32 {
    fn from(t: ControlMode) -> i32 {
        t as i32
    }
}

impl From<i64> for ControlMode {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
