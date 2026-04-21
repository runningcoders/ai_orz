//! Tool call logging implementation using daily JSONL files
//!
//! Stores tool call traces at: {base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl
//! Each line is a single ToolCallEntry with full input/output metadata

use std::path::PathBuf;

use anyhow::Result;
use once_cell::sync::OnceCell;

use super::entry::ToolCallEntry;
use crate::pkg::daily_jsonl::DailyJsonlWriter;

/// Global ToolCallLogger singleton
static INSTANCE: OnceCell<ToolCallLogger> = OnceCell::new();

/// ToolCallLogger is a factory that provides daily JSONL writers for tool call tracing
/// 
/// This is a singleton - initialize once with base data path at application startup,
/// then get the global instance anywhere with `ToolCallLogger::get()`.
#[derive(Debug, Clone)]
pub struct ToolCallLogger {
    base_data_path: PathBuf,
}

impl ToolCallLogger {
    /// Initialize the global ToolCallLogger singleton
    /// Must be called once at application startup
    pub fn init(base_data_path: PathBuf) {
        INSTANCE.set(Self { base_data_path })
            .expect("ToolCallLogger already initialized");
    }

    /// Get the global ToolCallLogger singleton instance
    /// Panics if not initialized yet
    pub fn get() -> &'static Self {
        INSTANCE.get().expect("ToolCallLogger not initialized")
    }

    /// Create a new ToolCallLogger instance (for direct use, prefer singleton)
    #[allow(dead_code)]
    pub fn new(base_data_path: PathBuf) -> Self {
        Self { base_data_path }
    }

    /// Get the writer for a specific tool's call traces
    pub fn writer_for_tool(&self, tool_id: &str) -> DailyJsonlWriter {
        let path = self.base_data_path
            .join("tools")
            .join(tool_id)
            .join("call_trace");
        DailyJsonlWriter::new(path)
    }

    /// Log a tool call entry to the daily JSONL file
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
