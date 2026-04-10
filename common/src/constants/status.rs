//! 状态枚举定义

#![cfg_attr(feature = "rusqlite", allow(unused_imports))]

#[cfg(feature = "rusqlite")]
use rusqlite::{
    types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef},
    Result as RusqliteResult,
};

/// AgentPo 状态枚举（用于软删除）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentPoStatus {
    /// 已删除（软删除）
    Deleted = 0,
    /// 正常可用
    Normal = 1,
}

impl AgentPoStatus {
    /// 从 i32 转换为状态枚举
    ///
    /// 0 表示 Deleted，其他值都表示 Normal
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            _ => Self::Normal,
        }
    }

    /// 将状态转换为 i32 存储
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for AgentPoStatus {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for AgentPoStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}

impl Default for AgentPoStatus {
    fn default() -> Self {
        Self::Normal
    }
}

/// ModelProviderPo 状态枚举（用于软删除）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModelProviderPoStatus {
    /// 已删除（软删除）
    Deleted = 0,
    /// 正常可用
    Normal = 1,
}

impl ModelProviderPoStatus {
    /// 从 i32 转换为状态枚举
    ///
    /// 0 表示 Deleted，其他值都表示 Normal
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            _ => Self::Normal,
        }
    }

    /// 将状态转换为 i32 存储
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for ModelProviderPoStatus {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for ModelProviderPoStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}

impl Default for ModelProviderPoStatus {
    fn default() -> Self {
        Self::Normal
    }
}

/// Organization 状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OrganizationStatus {
    /// 已禁用/已删除
    Disabled = 0,
    /// 正常可用
    Active = 1,
}

impl OrganizationStatus {
    /// 从 i32 转换
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Disabled,
            _ => Self::Active,
        }
    }

    /// 转换为 i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for OrganizationStatus {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for OrganizationStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}

impl Default for OrganizationStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// User 状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UserStatus {
    /// 已禁用
    Disabled = 0,
    /// 正常可用
    Active = 1,
}

impl UserStatus {
    /// 从 i32 转换
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Disabled,
            _ => Self::Active,
        }
    }

    /// 转换为 i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for UserStatus {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for UserStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}

impl Default for UserStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Organization 范围枚举（区分本地/远程组织，用于多节点网络扩展）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OrganizationScope {
    /// 本地组织（运行在当前设备）
    Local = 0,
    /// 远程组织（其他网络节点）
    Remote = 1,
}

impl OrganizationScope {
    /// 从 i32 转换
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Local,
            _ => Self::Remote,
        }
    }

    /// 转换为 i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for OrganizationScope {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for OrganizationScope {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}

impl Default for OrganizationScope {
    fn default() -> Self {
        Self::Local
    }
}

/// Message 发送者角色枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MessageRole {
    /// 用户发送
    User = 0,
    /// Agent 发送
    Agent = 1,
    /// 系统消息
    System = 2,
}

impl MessageRole {
    /// 从 i32 转换
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::User,
            1 => Self::Agent,
            _ => Self::System,
        }
    }

    /// 转换为 i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for MessageRole {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for MessageRole {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}

impl Default for MessageRole {
    fn default() -> Self {
        Self::User
    }
}

/// Message 类型枚举（区分存储方式）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MessageType {
    /// 纯文本消息，content 存储完整文本
    Text = 0,
    /// 图片消息，content 存储文件相对路径，meta_json 存储元数据
    Image = 1,
    /// 文件/文档消息，content 存储文件相对路径，meta_json 存储元数据
    File = 2,
    /// 音频消息，content 存储文件相对路径，meta_json 存储元数据
    Audio = 3,
    /// 视频消息，content 存储文件相对路径，meta_json 存储元数据
    Video = 4,
}

impl MessageType {
    /// 从 i32 转换
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Text,
            1 => Self::Image,
            2 => Self::File,
            3 => Self::Audio,
            _ => Self::Video,
        }
    }

    /// 转换为 i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for MessageType {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for MessageType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}

impl Default for MessageType {
    fn default() -> Self {
        Self::Text
    }
}

/// Message 处理状态枚举（用于事件总线处理状态跟踪）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MessageStatus {
    /// 待处理（等待被消费
    Pending = 0,
    /// 处理中（已被消费，正在处理
    Processing = 1,
    /// 已完成（处理完成，确认完成
    Completed = 2,
    /// 处理失败，需要重试或人工处理
    Failed = 3,
}

impl MessageStatus {
    /// 从 i32 转换
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Pending,
            1 => Self::Processing,
            2 => Self::Completed,
            _ => Self::Failed,
        }
    }

    /// 转换为 i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for MessageStatus {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_i32()))
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for MessageStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        Ok(Self::from_i32(v as i32))
    }
}

impl Default for MessageStatus {
    fn default() -> Self {
        Self::Pending
    }
}
