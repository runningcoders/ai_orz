//! Tool call tracing entry definition

use serde::{Deserialize, Serialize};

/// Status of a tool call
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolCallStatus {
    /// Tool invocation has started (for self-scheduled tools)
    Started,
    /// Tool invocation completed successfully
    Completed,
    /// Tool invocation failed with error
    Failed,
}

/// A single tool call entry logged to JSONL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallEntry {
    /// Unique call ID
    pub call_id: String,
    /// Tool ID that was called
    pub tool_id: String,
    /// Tool name (for easier querying)
    pub tool_name: String,
    /// Agent ID that initiated this call (optional)
    pub agent_id: Option<String>,
    /// Task ID this call is associated with (optional)
    pub task_id: Option<String>,
    /// Project ID this call is associated with (optional)
    pub project_id: Option<String>,
    /// Start timestamp (unix millis)
    pub started_at: u64,
    /// Finish timestamp (unix millis)
    pub finished_at: u64,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Input arguments as JSON (serialized)
    pub input: serde_json::Value,
    /// Output result as JSON (serialized)
    pub output: Option<serde_json::Value>,
    /// Error message if call failed
    pub error: Option<String>,
    /// Call status
    pub status: ToolCallStatus,
    /// Additional arbitrary metadata
    pub metadata: serde_json::Value,
}

impl Default for ToolCallEntry {
    fn default() -> Self {
        Self {
            call_id: String::new(),
            tool_id: String::new(),
            tool_name: String::new(),
            agent_id: None,
            task_id: None,
            project_id: None,
            started_at: 0,
            finished_at: 0,
            duration_ms: 0,
            input: serde_json::Value::Null,
            output: None,
            error: None,
            status: ToolCallStatus::Started,
            metadata: serde_json::Value::Null,
        }
    }
}

// ToolCallStatus → DTO 互转
impl From<ToolCallStatus> for common::api::tool::ToolCallStatusDto {
    fn from(s: ToolCallStatus) -> Self {
        match s {
            ToolCallStatus::Started => common::api::tool::ToolCallStatusDto::Started,
            ToolCallStatus::Completed => common::api::tool::ToolCallStatusDto::Completed,
            ToolCallStatus::Failed => common::api::tool::ToolCallStatusDto::Failed,
        }
    }
}

// ToolCallEntry → DTO：字段完全 1:1，仅 status 做 enum 转换
impl From<ToolCallEntry> for common::api::tool::ToolCallEntryDetail {
    fn from(entry: ToolCallEntry) -> Self {
        Self {
            call_id: entry.call_id,
            tool_id: entry.tool_id,
            tool_name: entry.tool_name,
            agent_id: entry.agent_id,
            task_id: entry.task_id,
            project_id: entry.project_id,
            started_at: entry.started_at,
            finished_at: entry.finished_at,
            duration_ms: entry.duration_ms,
            input: entry.input,
            output: entry.output,
            error: entry.error,
            status: entry.status.into(),
            metadata: entry.metadata,
        }
    }
}

/// 对工具 trace 的 input/output/error 进行**字段级脱敏**（所有协议统一策略）
///
/// 保留完整 JSON 结构，仅把命中敏感字段（password / api_key / token / secret /
/// authorization / credential 等）的值递归替换为 `***`；错误文本中的 KV 敏感模式
/// 也做同级别脱敏。
///
/// 不再按协议类型（Builtin / Http / Mcp）做区分，全量工具统一脱敏：一方面避免
/// 运行时 ToolPo 的 protocol 字段与实际不符导致 Builtin 意外漏脱敏，另一方面
/// Builtin 工具（如 shell_exec 的命令参数）本身也可能携带敏感信息，统一处理更安全。
///
/// 历史：最早对 Http/Mcp 做 fail-closed 全量替换 `[REDACTED]`（导致前端看不到任何
/// 内容）→ 改为字段级脱敏但仍按协议跳过 Builtin → 本次彻底去掉协议分支，所有工具
/// 一视同仁。
pub(crate) fn redact_trace_values_for_tool(
    _po: &crate::models::tool::ToolPo,
    mut input: serde_json::Value,
    mut output: Option<serde_json::Value>,
    error: Option<String>,
) -> (serde_json::Value, Option<serde_json::Value>, Option<String>) {
    crate::pkg::logging::mask_sensitive_json(&mut input);
    if let Some(ref mut v) = output {
        crate::pkg::logging::mask_sensitive_json(v);
    }
    let error = error.map(|e| crate::pkg::logging::mask_sensitive_text(&e));

    (input, output, error)
}
