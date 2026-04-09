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
