//! Tool call logging implementation using daily JSONL files
//!
//! Stores tool call traces at: {base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl
//! Each line is a single ToolCallEntry with full input/output metadata

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::Result;
use once_cell::sync::OnceCell;

use super::entry::{ToolCallEntry, ToolCallStatus};
use crate::pkg::daily_jsonl::DailyJsonlWriter;

pub const MAX_TOOL_CALL_QUERY_LIMIT: usize = 100;

/// Query filters for tool call trace entries.
#[derive(Debug, Clone, Default)]
pub struct ToolCallQuery {
    pub call_id: Option<String>,
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub tool_id: Option<String>,
    pub status: Option<ToolCallStatus>,
    pub started_after: Option<u64>,
    pub started_before: Option<u64>,
    /// Default: latest one matching entry.
    pub limit: Option<usize>,
}

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
        let _ = INSTANCE.get_or_init(|| Self { base_data_path });
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
        DailyJsonlWriter::new(self.trace_dir_for_tool(tool_id))
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

    /// Read a logged tool call entry by call ID.
    ///
    /// If `tool_id` is provided, only that tool's trace directory is scanned.
    /// Otherwise all tool trace directories under `{base_data_path}/tools` are scanned.
    pub fn read_call_by_id(
        &self,
        tool_id: Option<&str>,
        call_id: &str,
    ) -> Result<Option<ToolCallEntry>> {
        let entries = self.query_calls(ToolCallQuery {
            call_id: Some(call_id.to_string()),
            tool_id: tool_id.map(ToString::to_string),
            limit: Some(1),
            ..Default::default()
        })?;
        Ok(entries.into_iter().next())
    }

    /// Query logged tool call entries by common fields.
    ///
    /// First implementation intentionally scans JSONL files directly. It is
    /// simple and storage-detail-local; if query volume grows, add an index
    /// without changing this public API.
    pub fn query_calls(&self, query: ToolCallQuery) -> Result<Vec<ToolCallEntry>> {
        let tool_ids = self.resolve_tool_ids(query.tool_id.as_deref())?;
        let mut entries = Vec::new();

        for tool_id in tool_ids {
            let trace_dir = self.trace_dir_for_tool(&tool_id);
            if !trace_dir.exists() {
                continue;
            }
            for path in jsonl_files(&trace_dir)? {
                read_matching_entries(&path, &query, &mut entries)?;
            }
        }

        entries.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.finished_at.cmp(&a.finished_at))
                .then_with(|| b.call_id.cmp(&a.call_id))
        });

        let limit = query.limit.unwrap_or(1);
        if limit > MAX_TOOL_CALL_QUERY_LIMIT {
            anyhow::bail!(
                "tool call query limit exceeds maximum {}",
                MAX_TOOL_CALL_QUERY_LIMIT
            );
        }
        entries.truncate(limit);
        Ok(entries)
    }

    fn trace_dir_for_tool(&self, tool_id: &str) -> PathBuf {
        self.base_data_path
            .join("tools")
            .join(tool_id)
            .join("call_trace")
    }

    fn resolve_tool_ids(&self, tool_id: Option<&str>) -> Result<Vec<String>> {
        if let Some(tool_id) = tool_id {
            validate_tool_id_for_trace_path(tool_id)?;
            return Ok(vec![tool_id.to_string()]);
        }

        let tools_dir = self.base_data_path.join("tools");
        if !tools_dir.exists() {
            return Ok(Vec::new());
        }

        let mut tool_ids = Vec::new();
        for entry in fs::read_dir(tools_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if entry.path().join("call_trace").is_dir() {
                tool_ids.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        tool_ids.sort();
        Ok(tool_ids)
    }
}

fn validate_tool_id_for_trace_path(tool_id: &str) -> Result<()> {
    if tool_id.is_empty()
        || tool_id.contains('/')
        || tool_id.contains('\\')
        || tool_id == "."
        || tool_id == ".."
    {
        anyhow::bail!("invalid tool_id for call trace lookup");
    }
    Ok(())
}

fn jsonl_files(trace_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(trace_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "jsonl")
        {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

fn read_matching_entries(
    path: &Path,
    query: &ToolCallQuery,
    entries: &mut Vec<ToolCallEntry>,
) -> Result<()> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: ToolCallEntry = serde_json::from_str(&line)?;
        if matches_query(&entry, query) {
            entries.push(entry);
        }
    }
    Ok(())
}

fn matches_query(entry: &ToolCallEntry, query: &ToolCallQuery) -> bool {
    query
        .call_id
        .as_ref()
        .is_none_or(|call_id| entry.call_id == *call_id)
        && query
            .agent_id
            .as_ref()
            .is_none_or(|agent_id| entry.agent_id.as_deref() == Some(agent_id.as_str()))
        && query
            .project_id
            .as_ref()
            .is_none_or(|project_id| entry.project_id.as_deref() == Some(project_id.as_str()))
        && query
            .task_id
            .as_ref()
            .is_none_or(|task_id| entry.task_id.as_deref() == Some(task_id.as_str()))
        && query
            .tool_id
            .as_ref()
            .is_none_or(|tool_id| entry.tool_id == *tool_id)
        && query.status.is_none_or(|status| entry.status == status)
        && query
            .started_after
            .is_none_or(|started_after| entry.started_at >= started_after)
        && query
            .started_before
            .is_none_or(|started_before| entry.started_at <= started_before)
}
