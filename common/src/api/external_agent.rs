//! 外部 Agent API 类型
//!
//! 用于创建外部 Agent（Cli/Remote）的 HTTP 请求/响应 DTO。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 创建外部 Agent 请求
///
/// 通过 kind 字段区分 Cli / Remote 两种外部 Agent 类型，
/// 各 kind 对应的配置字段在 handler 内做必填校验。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateExternalAgentRequest {
    /// Agent 名称
    pub name: String,
    /// Agent 角色标签列表
    #[serde(default)]
    pub roles: Option<Vec<String>>,
    /// Agent 描述
    #[serde(default)]
    pub description: Option<String>,
    /// 能力列表
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
    /// Agent 灵魂提示词
    #[serde(default)]
    pub soul: Option<String>,
    /// Agent 类型：cli / remote
    pub kind: String,

    // ===== CLI 配置（kind=cli 时必填） =====
    /// 启动命令（如 "codex"、"claude"、"aider"）
    #[serde(default)]
    pub command: Option<String>,
    /// 命令参数
    #[serde(default)]
    pub args: Option<Vec<String>>,
    /// 工作目录（绝对路径）
    #[serde(default)]
    pub work_dir: Option<String>,
    /// 环境变量
    #[serde(default)]
    pub env: Option<Vec<(String, String)>>,
    /// 超时时间（秒），默认 300
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// 自定义 prompt 模板（使用 {prompt} 占位符）
    #[serde(default)]
    pub prompt_template: Option<String>,

    // ===== Remote 配置（kind=remote 时必填） =====
    /// A2A Server 的 base URL
    #[serde(default)]
    pub endpoint: Option<String>,
    /// 目标 Agent 名称（agents/sendTask 的 agent_id 参数）
    #[serde(default)]
    pub agent_name: Option<String>,
    /// 认证 token（Bearer）
    #[serde(default)]
    pub auth_token: Option<String>,
}

/// 创建外部 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateExternalAgentResponse {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// Agent 类型
    pub kind: String,
    /// 创建时间戳
    pub created_at: i64,
}
