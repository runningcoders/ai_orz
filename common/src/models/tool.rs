//! Common tool-related types.

use serde::{Deserialize, Serialize};

/// Lightweight tool call trace reference.
///
/// Points to a detailed tool execution trace stored in tool-specific storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallTraceRef {
    /// Tool ID that this call belongs to.
    pub tool_id: String,
    /// Unique call ID for this specific tool execution.
    pub call_id: String,
}

impl ToolCallTraceRef {
    /// Create a new ToolCallTraceRef.
    pub fn new(tool_id: String, call_id: String) -> Self {
        Self { tool_id, call_id }
    }
}