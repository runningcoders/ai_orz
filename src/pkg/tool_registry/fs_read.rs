//! Builtin read_file tool implementation

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use anyhow::{anyhow};
use async_trait::async_trait;
use common::error::{Error, Result};
use common::enums::{ControlMode, ToolProtocol};
use dyn_clone::clone_trait_object;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tracing::debug;

/// `read_file` (fs_read) tool parameter arguments
#[derive(Debug, Deserialize)]
pub struct ReadFileArgs {
    pub path: String,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub end_line: Option<usize>,
    #[serde(default)]
    pub grep: Option<String>,
    #[serde(default = "default_context_lines")]
    pub context_lines: usize,
}

fn default_context_lines() -> usize {
    2
}

/// Grep match result with context
#[derive(Debug, Serialize)]
pub struct GrepMatch {
    pub line_number: usize,
    pub content: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Factory for creating read_file builtin tool
#[derive(Debug, Clone, Default)]
pub struct FsReadToolFactory;

impl crate::pkg::tool_registry::BuiltinToolFactory for FsReadToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "fs_read".to_string(),
            name: "read_file".to_string(),
            description: concat!(
                "Read content from a file in the current project/workspace. ",
                "Supports full file read, range read by line numbers, and grep-style pattern matching. ",
                "Returns content with line numbers for easy editing. "
            ).to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read, relative to the project/workspace root"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Optional: start reading from this line (1-indexed). If omitted, start from beginning."
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Optional: stop reading at this line (inclusive). If omitted, read to end."
                    },
                    "grep": {
                        "type": "string",
                        "description": "Optional: return lines containing this substring (simple string matching, not regex). Returns matches with context."
                    },
                    "context_lines": {
                        "type": "integer",
                        "default": 2,
                        "description": "Optional: number of context lines to include before and after each grep match."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            })),
            config: Value::Null,
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(FsReadCoreTool { po })
    }
}

/// Core implementation of read_file tool
#[derive(Debug, Clone)]
pub struct FsReadCoreTool {
    po: ToolPo,
}

#[async_trait::async_trait]
impl CoreTool for FsReadCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // Parse arguments
        let args: ReadFileArgs = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid arguments: {}", e))
            .map_err(|e| common::error::Error::from(e))?;

        // Get base data path from global config
        let base_path = crate::config::get().base_data_path();
        let target_path = resolve_and_validate_path(&base_path, &args.path)?;

        // Check if file exists
        if !target_path.exists() {
            return Ok(serde_json::json!({
                "success": false,
                "error": "File not found"
            }));
        }

        // Check file size before opening
        let metadata = std::fs::metadata(&target_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file metadata: {}", sanitize_error(e)))
            .map_err(|e| common::error::Error::from(e))?;

        const DEFAULT_READ_MAX_BYTES: usize = 1 * 1024 * 1024; // 1MB
        const HARD_READ_MAX_BYTES: usize = 10 * 1024 * 1024; // 10MB

        let size = metadata.len() as usize;
        if size > HARD_READ_MAX_BYTES {
            return Ok(serde_json::json!({
                "success": false,
                "error": format!("File too large: {} bytes, maximum allowed is {} bytes", size, HARD_READ_MAX_BYTES)
            }));
        }

        // Open and read file line by line
        let file = File::open(&target_path)
            .map_err(|e| anyhow::anyhow!("Failed to open file: {}", sanitize_error(e)))
            .map_err(|e| common::error::Error::from(e))?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines()
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", sanitize_error(e)))
            .map_err(|e| common::error::Error::from(e))?;

        let total_lines = lines.len();

        // Handle grep mode
        if let Some(pattern) = args.grep {
            let matches = find_grep_matches(&lines, &pattern, args.context_lines);
            return Ok(serde_json::json!({
                "success": true,
                "path": args.path,
                "query": pattern,
                "total_matches": matches.len(),
                "matches": matches
            }));
        }

        // Handle range mode
        let (start, end) = resolve_line_range(args.start_line, args.end_line, total_lines);
        let selected_lines = &lines[start..end];

        // Format with line numbers
        let mut content = String::new();
        for (i, line) in selected_lines.iter().enumerate() {
            let line_num = start + i + 1; // 1-indexed
            content.push_str(&format!("{:>4}|{}\n", line_num, line));
        }

        Ok(serde_json::json!({
            "success": true,
            "path": args.path,
            "size_bytes": size,
            "total_lines": total_lines,
            "content": content
        }))
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

/// Resolve and validate the target path against sandbox restrictions
fn resolve_and_validate_path(base_path: &Path, user_path: &str) -> Result<PathBuf> {
    // 1. Check sensitive filename patterns first
    if is_sensitive_filename(user_path) {
        return Err(anyhow::anyhow!("Access denied: cannot read sensitive file").into());
    }

    // 2. Build absolute path
    let user_path = Path::new(user_path);
    let absolute_path = if user_path.is_absolute() {
        user_path.to_path_buf()
    } else {
        base_path.join(user_path)
    };

    // 3. Canonicalize to resolve .. and symlinks
    let canonical = absolute_path.canonicalize()
        .map_err(|_| anyhow::anyhow!("Failed to resolve path: file not found or permission denied"))
        .map_err(|e| common::error::Error::from(e))?;

    // 4. Check that canonical path is still under base_path
    let base_canonical = base_path.canonicalize()
        .map_err(|e| anyhow::anyhow!("Invalid base data path: {}", e))
        .map_err(|e| common::error::Error::from(e))?;

    if !canonical.starts_with(&base_canonical) {
        return Err(anyhow::anyhow!("Access denied: path outside allowed workspace directory").into());
    }

    // 5. Reject symlinks
    if canonical.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(true)
    {
        return Err(anyhow::anyhow!("Access denied: symbolic links are not allowed").into());
    }

    Ok(canonical)
}

/// Check if filename matches sensitive patterns
fn is_sensitive_filename(path: &str) -> bool {
    let lower = path.to_lowercase();
    // Sensitive patterns
    if lower.contains(".env") { return true; }
    if lower.contains(".pem") { return true; }
    if lower.contains(".key") { return true; }
    if lower.contains(".p12") { return true; }
    if lower.contains(".pfx") { return true; }
    if lower.contains("id_rsa") { return true; }
    if lower.contains("id_dsa") { return true; }
    if lower.contains("id_ecdsa") { return true; }
    if lower.contains("password") { return true; }
    if lower.contains("secret") { return true; }
    if lower.contains("token") { return true; }
    if lower.contains("credential") { return true; }
    if lower.contains("auth") { return true; }
    // Hidden files starting with .
    if path.split('/').last()
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
    {
        return true;
    }
    false
}

/// Resolve start/end line numbers to range indices (0-based for slicing)
fn resolve_line_range(start: Option<usize>, end: Option<usize>, total: usize) -> (usize, usize) {
    let start = start.map(|s| s.saturating_sub(1)).unwrap_or(0);
    let end = end.unwrap_or(total);
    let end = end.min(total);
    if start >= end {
        (start, start) // empty range
    } else {
        (start, end)
    }
}

/// Find all lines containing the pattern, with context
fn find_grep_matches(lines: &[String], pattern: &str, context_lines: usize) -> Vec<GrepMatch> {
    let mut matches = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1; // 1-indexed
        if line.contains(pattern) {
            // Collect context before
            let context_before_start = idx.saturating_sub(context_lines);
            let context_before: Vec<String> = lines[context_before_start..idx]
                .iter()
                .enumerate()
                .map(|(i, content)| format!("{:>4}|{}", context_before_start + i + 1, content))
                .collect();

            // Collect context after
            let context_after_end = (idx + 1 + context_lines).min(lines.len());
            let context_after: Vec<String> = lines[idx + 1..context_after_end]
                .iter()
                .enumerate()
                .map(|(i, content)| format!("{:>4}|{}", idx + 1 + i + 1, content))
                .collect();

            matches.push(GrepMatch {
                line_number: line_num,
                content: format!("{:>4}|{}", line_num, line),
                context_before,
                context_after,
            });
        }
    }

    matches
}

/// Sanitize IO error to remove absolute paths
fn sanitize_error<E: std::fmt::Display>(e: E) -> String {
    let s = e.to_string();
    // Remove absolute path prefixes, keep only the error message
    // This is a simple sanitization, enough for our purposes
    s.split('/')
        .last()
        .map(|last| last.to_string())
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::write;
    use crate::pkg::request_context::RequestContext;

    #[tokio::test]
    async fn test_read_full_file_ok() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        write(&file_path, content).unwrap();

        // For testing, we call directly with absolute path inside temp dir
        let result = resolve_and_validate_path(dir.path(), file_path.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_sensitive_filename() {
        assert!(is_sensitive_filename(".env"));
        assert!(is_sensitive_filename("secret.key"));
        assert!(is_sensitive_filename("id_rsa"));
        assert!(is_sensitive_filename(".gitignore")); // hidden file
        assert!(!is_sensitive_filename("src/main.rs"));
        assert!(!is_sensitive_filename("README.md"));
    }

    #[test]
    fn test_resolve_line_range() {
        let total = 10;

        // full range
        assert_eq!(resolve_line_range(None, None, total), (0, 10));

        // start from 1 (0-based 0) to end
        assert_eq!(resolve_line_range(Some(1), None, total), (0, 10));

        // start 3, end 5 -> 2..5 (0-based)
        assert_eq!(resolve_line_range(Some(3), Some(5), total), (2, 5));

        // start > end -> empty
        assert_eq!(resolve_line_range(Some(8), Some(5), total), (7, 7));

        // end larger than total -> clamp
        assert_eq!(resolve_line_range(None, Some(20), total), (0, 10));
    }

    #[test]
    fn test_find_grep_matches() {
        let lines = vec![
            "first line".to_string(),
            "second line with match".to_string(),
            "third line".to_string(),
            "fourth line with match".to_string(),
            "fifth line".to_string(),
        ];

        let matches = find_grep_matches(&lines, "match", 1);
        assert_eq!(matches.len(), 2);

        // first match at line 2
        assert_eq!(matches[0].line_number, 2);
        assert_eq!(matches[0].context_before.len(), 1);
        assert_eq!(matches[0].context_after.len(), 1);

        // second match at line 4
        assert_eq!(matches[1].line_number, 4);
    }
}
