//! Tool call logging implementation using daily JSONL files
//!
//! Stores tool call traces at: {base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl
//! Each line is a single ToolCallEntry with full input/output metadata

use std::path::{Path, PathBuf};

use anyhow::Result;
use common::config::AppConfig as Config;

use super::entry::ToolCallEntry;
use crate::pkg::daily_jsonl::DailyJsonlWriter;

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
    #[allow(dead_code)]
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
