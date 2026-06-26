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
