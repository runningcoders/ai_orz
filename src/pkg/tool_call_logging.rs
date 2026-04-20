//! Tool call logging implementation using daily JSONL files
//!
//! Stores tool call traces at: {base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl
//! Each line is a single ToolCallEntry with full input/output metadata

use std::path::{Path, PathBuf};

use anyhow::Result;
use common::config::AppConfig as Config;
use serde::{Deserialize, Serialize};

use super::daily_jsonl::DailyJsonlWriter;

/// A single tool call entry logged to JSONL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallEntry {
    /// Unique call ID
    pub call_id: String,
    /// Tool ID that was called
    pub tool_id: String,
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
    /// Additional arbitrary metadata
    pub metadata: serde_json::Value,
}

/// Tool call logger that writes to daily partitioned JSONL
#[derive(Debug, Clone)]
pub struct ToolCallLogger {
    base_data_path: PathBuf,
}

impl ToolCallLogger {
    /// Create a new ToolCallLogger from the global config
    pub fn new(config: &Config) -> Self {
        Self {
            base_data_path: PathBuf::from(config.base_data_path.clone()),
        }
    }

    /// Create a new ToolCallLogger with explicit base path
    #[allow(dead_code)]
    pub fn with_base_path(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_data_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Get the writer for a specific tool's call traces
    pub fn writer_for_tool(&self, tool_id: &str) -> DailyJsonlWriter {
        let path = self
            .base_data_path
            .join("tools")
            .join(tool_id)
            .join("call_trace");
        DailyJsonlWriter::new(path)
    }

    /// Log a tool call entry
    /// Returns (date_path, line_number) for the logged entry
    pub fn log_call(&self, tool_id: &str, entry: ToolCallEntry) -> Result<(String, usize)> {
        let writer = self.writer_for_tool(tool_id);
        writer.append(&entry)
    }

    /// Read a logged tool call entry by date and line number
    pub fn read_call(
        &self,
        tool_id: &str,
        date: &str,
        line_number: usize,
    ) -> Result<ToolCallEntry> {
        let writer = self.writer_for_tool(tool_id);
        writer.read_line_json(date, line_number)
    }
}