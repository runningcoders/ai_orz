//! MCP Server 实体
//!
//! `McpServerPo` 只表示 MCP Server 连接配置的持久化数据；
//! MCP client/session 生命周期不在这里管理，后续由 MCP ToolCall 实现内聚处理。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const REDACTED_CONFIG_VALUE: &str = "[REDACTED]";

#[cfg(test)]
#[path = "mcp_server_test.rs"]
mod mcp_server_test;

/// MCP Server transport 类型。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "INTEGER")]
#[derive(Default)]
pub enum McpTransport {
    /// stdio transport: command + args，不走 shell 拼接。
    #[default]
    Stdio = 0,
    /// Streamable HTTP transport.
    StreamableHttp = 1,
}

impl From<i32> for McpTransport {
    fn from(v: i32) -> Self {
        match v {
            1 => Self::StreamableHttp,
            _ => Self::Stdio,
        }
    }
}

impl From<i64> for McpTransport {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl From<McpTransport> for i32 {
    fn from(v: McpTransport) -> Self {
        v as i32
    }
}

impl McpTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp => "streamable_http",
        }
    }
}

/// MCP Server 状态。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "INTEGER")]
#[derive(Default)]
pub enum McpServerStatus {
    /// 已删除（软删除）。
    Deleted = 0,
    /// 启用。
    #[default]
    Enabled = 1,
    /// 禁用。
    Disabled = 2,
}

impl From<i32> for McpServerStatus {
    fn from(v: i32) -> Self {
        match v {
            1 => Self::Enabled,
            2 => Self::Disabled,
            _ => Self::Deleted,
        }
    }
}

impl From<i64> for McpServerStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl From<McpServerStatus> for i32 {
    fn from(v: McpServerStatus) -> Self {
        v as i32
    }
}

/// MCP Server 连接配置。
///
/// 注意：这里是持久化配置原文，管理面展示/日志/错误输出必须另行脱敏。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerConfig {
    /// stdio transport command。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// stdio transport args。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// stdio transport 显式环境变量；默认不继承系统环境。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// streamable HTTP URL。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// streamable HTTP headers。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// 调用超时。
    pub timeout_ms: u64,
    /// 连接超时。
    pub connect_timeout_ms: u64,
    /// 最大响应体大小。
    pub response_max_bytes: u64,
}

impl McpServerConfig {
    pub fn default_stdio() -> Self {
        Self {
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            url: None,
            headers: BTreeMap::new(),
            timeout_ms: 30_000,
            connect_timeout_ms: 10_000,
            response_max_bytes: 10 * 1024 * 1024,
        }
    }

    pub fn default_streamable_http() -> Self {
        Self {
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            url: None,
            headers: BTreeMap::new(),
            timeout_ms: 30_000,
            connect_timeout_ms: 10_000,
            response_max_bytes: 10 * 1024 * 1024,
        }
    }

    pub fn redacted_for_management(&self) -> Self {
        let mut config = self.clone();
        config.env = config
            .env
            .keys()
            .map(|key| (key.clone(), REDACTED_CONFIG_VALUE.to_string()))
            .collect();
        config.headers = config
            .headers
            .keys()
            .map(|key| (key.clone(), REDACTED_CONFIG_VALUE.to_string()))
            .collect();
        config.url = config.url.as_deref().map(redact_url_for_management);
        config
    }
}

fn redact_url_for_management(url: &str) -> String {
    let url = redact_url_userinfo(url);
    redact_url_query(&url)
}

fn redact_url_userinfo(url: &str) -> String {
    let Some(authority_start) = url.find("://").map(|idx| idx + 3) else {
        return url.to_string();
    };

    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map(|idx| authority_start + idx)
        .unwrap_or(url.len());
    let Some(userinfo_end) = url[authority_start..authority_end]
        .rfind('@')
        .map(|idx| authority_start + idx)
    else {
        return url.to_string();
    };

    format!(
        "{}{}{}",
        &url[..authority_start],
        REDACTED_CONFIG_VALUE,
        &url[userinfo_end..]
    )
}

fn redact_url_query(url: &str) -> String {
    let Some(query_start) = url.find('?') else {
        return url.to_string();
    };

    let fragment_start = url[query_start + 1..]
        .find('#')
        .map(|idx| query_start + 1 + idx);

    match fragment_start {
        Some(fragment_start) => format!(
            "{}?{}{}",
            &url[..query_start],
            REDACTED_CONFIG_VALUE,
            &url[fragment_start..]
        ),
        None => format!("{}?{}", &url[..query_start], REDACTED_CONFIG_VALUE),
    }
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self::default_stdio()
    }
}

/// MCP Server 业务实体。
///
/// 上层 Domain/Handler 应使用该业务实体；`McpServerPo` 仅作为 DAL/DAO 内部的持久化细节。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub po: McpServerPo,
}

impl McpServer {
    pub fn new(
        id: String,
        name: String,
        transport: McpTransport,
        config: McpServerConfig,
        creator: Option<String>,
    ) -> Self {
        Self {
            po: McpServerPo::new(id, name, transport, config, creator),
        }
    }

    pub fn from_po(po: McpServerPo) -> Self {
        Self { po }
    }

    pub fn redacted_for_management(mut self) -> Self {
        let config = self.po.config().redacted_for_management();
        self.po.set_config(&config);
        self
    }
}

/// MCP Server 通用查询条件。
#[derive(Debug, Clone, Default)]
pub struct McpServerQuery {
    pub id: Option<String>,
    pub name: Option<String>,
    pub transport: Option<McpTransport>,
    pub status: Option<McpServerStatus>,
    pub exclude_status: Option<McpServerStatus>,
    pub pagination: common::api::PaginationParams,
}

/// MCP Server 持久化对象。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct McpServerPo {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    /// JSON serialized `McpServerConfig`。
    pub config: String,
    pub status: McpServerStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
}

impl McpServerPo {
    pub fn new(
        id: String,
        name: String,
        transport: McpTransport,
        config: McpServerConfig,
        creator: Option<String>,
    ) -> Self {
        let id = if id.is_empty() {
            Uuid::now_v7().to_string()
        } else {
            id
        };
        let now = common::constants::utils::current_timestamp();
        Self {
            id,
            name,
            transport,
            config: serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string()),
            status: McpServerStatus::Enabled,
            created_at: now,
            updated_at: now,
            created_by: creator.clone(),
            updated_by: creator,
        }
    }

    pub fn config(&self) -> McpServerConfig {
        serde_json::from_str(&self.config).unwrap_or_default()
    }

    pub fn set_config(&mut self, config: &McpServerConfig) {
        self.config = serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());
    }

    pub fn touch(&mut self, modifier: Option<String>) {
        self.updated_at = common::constants::utils::current_timestamp();
        self.updated_by = modifier;
    }
}
