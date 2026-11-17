//! Tool call tracing - unified module for tool call logging
//!
//! This module provides:
//! - ToolCallEntry/ToolCallStatus: Structured logging entry with call status
//! - ToolCallLogger: Daily JSONL based logger for persistent tool call history
//! - redact_trace_values_for_tool: Trace value redaction for HTTP/MCP tools

pub mod entry;
pub mod logger;
pub(crate) mod sidecar_index;

#[cfg(test)]
mod tests;
