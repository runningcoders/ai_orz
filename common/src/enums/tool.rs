//! Tool 相关枚举

use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;
use std::fmt;

/// 工具协议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "TEXT", rename_all = "lowercase"))]
pub enum ToolProtocol {
    /// 内置工具（代码中实现）
    #[default]
    Builtin,
    /// HTTP 远程调用工具
    Http,
    /// MCP (Model Context Protocol) 工具
    Mcp,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "TEXT", rename_all = "lowercase"))]
pub enum ToolStatus {
    /// 启用
    #[default]
    Enabled,
    /// 禁用
    Disabled,
}

impl fmt::Display for ToolStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolStatus::Enabled => write!(f, "enabled"),
            ToolStatus::Disabled => write!(f, "disabled"),
        }
    }
}
