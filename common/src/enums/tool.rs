//! Tool 相关枚举

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;
use std::fmt;

/// 工具协议类型
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ToolProtocol {
    /// 内置工具（代码中实现）
    #[default]
    Builtin = 0,
    /// HTTP 远程调用工具
    Http = 1,
    /// MCP (Model Context Protocol) 工具
    Mcp = 2,
}

impl From<i32> for ToolProtocol {
    fn from(v: i32) -> Self {
        match v {
            0 => ToolProtocol::Builtin,
            1 => ToolProtocol::Http,
            2 => ToolProtocol::Mcp,
            _ => ToolProtocol::Builtin,
        }
    }
}

impl From<i64> for ToolProtocol {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl ToolProtocol {
    /// Convert the protocol type to i32 for database storage.
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl fmt::Display for ToolProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolProtocol::Builtin => write!(f, "builtin"),
            ToolProtocol::Http => write!(f, "http"),
            ToolProtocol::Mcp => write!(f, "mcp"),
        }
    }
}

/// 工具状态
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ToolStatus {
    /// 禁用
    Disabled = 0,
    /// 启用
    #[default]
    Enabled = 1,
    /// 同步异常：远端工具已消失/改名，本地记录与绑定保留但正常业务不可用
    Stale = 2,
}

impl From<i32> for ToolStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => ToolStatus::Disabled,
            1 => ToolStatus::Enabled,
            2 => ToolStatus::Stale,
            _ => ToolStatus::Enabled,
        }
    }
}

impl From<i64> for ToolStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl ToolStatus {
    /// Convert the tool status to i32 for database storage.
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl fmt::Display for ToolStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolStatus::Enabled => write!(f, "enabled"),
            ToolStatus::Disabled => write!(f, "disabled"),
            ToolStatus::Stale => write!(f, "stale"),
        }
    }
}

/// Control mode (工具控制模式：rig自动处理 / 自建链路手动处理)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ControlMode {
    /// Auto (rig 原生自动处理，适合简单工具)
    #[default]
    Auto = 0,
    /// Manual (自建链路处理，需要收敛控制的关键工具)
    Manual = 1,
}

impl fmt::Display for ControlMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlMode::Auto => write!(f, "auto"),
            ControlMode::Manual => write!(f, "manual"),
        }
    }
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
