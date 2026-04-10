//! Message related enums

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

/// Message role (谁发送的消息)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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

impl ToSql for MessageRole {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        Ok(ToSqlOutput::from(*self as i32))
    }
}

impl FromSql for MessageRole {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        i32::column_result(value).map(|v| v.into())
    }
}

/// Message type (消息类型)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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
}

impl From<i32> for MessageType {
    fn from(v: i32) -> Self {
        match v {
            0 => MessageType::Text,
            1 => MessageType::Image,
            2 => MessageType::File,
            3 => MessageType::Audio,
            4 => MessageType::Video,
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

impl ToSql for MessageType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        Ok(ToSqlOutput::from(*self as i32))
    }
}

impl FromSql for MessageType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        i32::column_result(value).map(|v| v.into())
    }
}

/// Message status (处理状态，用于事件总线恢复)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MessageStatus {
    /// Pending (待处理)
    #[default]
    Pending = 0,
    /// Processing (处理中)
    Processing = 1,
    /// Processed (处理完成)
    Processed = 2,
    /// Failed (处理失败)
    Failed = 3,
}

impl From<i32> for MessageStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => MessageStatus::Pending,
            1 => MessageStatus::Processing,
            2 => MessageStatus::Processed,
            3 => MessageStatus::Failed,
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

impl ToSql for MessageStatus {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        Ok(ToSqlOutput::from(*self as i32))
    }
}

impl FromSql for MessageStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        i32::column_result(value).map(|v| v.into())
    }
}
