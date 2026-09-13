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
    /// 环境变量，形如 `[["KEY", "VALUE"], ...]`
    ///
    /// ⚠️ schema 必须用 `Vec<Vec<String>>` 表达，不能让它按真实类型 `Vec<(String, String)>` 生成：
    /// schemars 对元组会产出 `"items": [ {...}, {...} ], "minItems": 2, "maxItems": 2`
    /// 这种 draft-07 元组校验写法，而 OpenAI 兼容网关（火山方舟/豆包等）只接受
    /// `items` 为**对象**的 JSON Schema。一旦某个工具的 `parameters` 里出现数组形式的 `items`，
    /// 网关会对**整个 chat/completions 请求**报 `400 InvalidParameter` —— 即「毒工具」：
    /// 只要该工具在册，持有它的 Agent 的**每一轮**模型调用都会失败，且错误信息完全指不出是哪个工具。
    /// `Vec<Vec<String>>` 与 `Vec<(String, String)>` 的 wire format 同构（都是 `[["K","V"]]`），
    /// 故只影响 schema 生成、不影响反序列化与线上契约。
    #[serde(default)]
    #[schemars(with = "Option<Vec<Vec<String>>>")]
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
