//! Agent 类型枚举：决定 Agent 的执行后端

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Agent 类型：决定 Agent 的执行后端
///
/// - Local: ai_orz 内部 Brain + Tools 执行
/// - Cli: CLI 子进程包装（如 Codex / Claude Code / Aider）
/// - Remote: 远程 A2A 协议 Agent
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum AgentKind {
    /// 本地 Agent（ai_orz 内部 Brain 执行）
    #[default]
    Local = 0,
    /// CLI Agent（子进程包装，如 Codex / Claude Code）
    Cli = 1,
    /// 远程 Agent（通过 A2A 协议调用的外部 Agent）
    Remote = 2,
}

impl AgentKind {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Cli,
            2 => Self::Remote,
            _ => Self::Local,
        }
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }

    /// 是否为外部 Agent（需要外部执行器）
    pub fn is_external(&self) -> bool {
        matches!(self, Self::Cli | Self::Remote)
    }

    /// 是否为本地 Agent
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    /// 是否为 CLI Agent
    pub fn is_cli(&self) -> bool {
        matches!(self, Self::Cli)
    }

    /// 是否为远程 Agent
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote)
    }

    /// 转换为字符串标识（用于 API 响应 / 前端展示）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cli => "cli",
            Self::Remote => "remote",
        }
    }

    /// 从字符串解析（不区分大小写）
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cli" => Self::Cli,
            "remote" => Self::Remote,
            _ => Self::Local,
        }
    }
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<i32> for AgentKind {
    fn from(v: i32) -> Self {
        Self::from_i32(v)
    }
}

impl From<i64> for AgentKind {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_local() {
        assert_eq!(AgentKind::default(), AgentKind::Local);
    }

    #[test]
    fn test_is_external() {
        assert!(!AgentKind::Local.is_external());
        assert!(AgentKind::Cli.is_external());
        assert!(AgentKind::Remote.is_external());
    }

    #[test]
    fn test_is_local_cli_remote() {
        assert!(AgentKind::Local.is_local());
        assert!(!AgentKind::Local.is_cli());
        assert!(!AgentKind::Local.is_remote());

        assert!(!AgentKind::Cli.is_local());
        assert!(AgentKind::Cli.is_cli());
        assert!(!AgentKind::Cli.is_remote());

        assert!(!AgentKind::Remote.is_local());
        assert!(!AgentKind::Remote.is_cli());
        assert!(AgentKind::Remote.is_remote());
    }

    #[test]
    fn test_from_i32() {
        assert_eq!(AgentKind::from(0), AgentKind::Local);
        assert_eq!(AgentKind::from(1), AgentKind::Cli);
        assert_eq!(AgentKind::from(2), AgentKind::Remote);
        assert_eq!(AgentKind::from(99), AgentKind::Local);
    }

    #[test]
    fn test_to_i32() {
        assert_eq!(AgentKind::Local.to_i32(), 0);
        assert_eq!(AgentKind::Cli.to_i32(), 1);
        assert_eq!(AgentKind::Remote.to_i32(), 2);
    }

    #[test]
    fn test_from_i64() {
        assert_eq!(AgentKind::from(1i64), AgentKind::Cli);
        assert_eq!(AgentKind::from(2i64), AgentKind::Remote);
    }
}
