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

// 边界决策（2026-09-03）：trace 落库保持原文，字段级脱敏已移除；
// 对外出口（tool-call-entries 查询接口）统一用 `redact!` 宏脱敏，引擎实现见 common::redaction。
