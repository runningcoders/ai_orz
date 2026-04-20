//! Tool 相关枚举

use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;
use std::fmt;

/// 工具协议类型
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ToolProtocol {
    /// 内置工具（代码中实现）
    Builtin = 0,
    /// HTTP 远程调用工具
    Http = 1,
    /// MCP (Model Context Protocol) 工具
    Mcp = 2,
}

impl Default for ToolProtocol {
    fn default() -> Self {
        ToolProtocol::Builtin
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ToolStatus {
    /// 禁用
    Disabled = 0,
    /// 启用
    Enabled = 1,
}

impl Default for ToolStatus {
    fn default() -> Self {
        ToolStatus::Enabled
    }
}

impl From<i32> for ToolStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => ToolStatus::Disabled,
            1 => ToolStatus::Enabled,
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
        }
    }
}
