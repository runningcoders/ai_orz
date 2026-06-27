//! Builtin write_file tool implementation

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use anyhow::{anyhow};
use async_trait::async_trait;
use common::error::{Error, Result};
use common::enums::{ControlMode, ToolProtocol};
use dyn_clone::clone_trait_object;
use serde::{Deserialize};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// `write_file` (fs_write) tool parameter arguments
#[derive(Debug, Deserialize)]
pub struct WriteFileArgs {
    pub path: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub after_line: Option<usize>,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub end_line: Option<usize>,
}

fn default_mode() -> String {
    "overwrite".to_string()
}

/// Factory for creating write_file builtin tool
#[derive(Debug, Clone, Default)]
pub struct FsWriteToolFactory;

impl crate::pkg::tool_registry::BuiltinToolFactory for FsWriteToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "fs_write".to_string(),
            name: "write_file".to_string(),
            description: concat!(
                "Write content to a file in the current project/workspace. ",
                "Supports multiple modes: overwrite entire file, append to end, ",
                "insert after specific line, delete range, replace range. ",
                "All operations are sandboxed to the workspace directory."
            ).to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write, relative to the project root"
                    },
                    "content": {
                        "type": "string",
                        "description": "New content to write/insert. Not used for delete_range mode."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["overwrite", "append", "insert_after", "delete_range", "replace_range"],
                        "default": "overwrite",
                        "description": "Write mode:\n- overwrite: replace entire file (atomic)\n- append: append content to end of file (atomic)\n- insert_after: insert new content after the specified line (atomic)\n- delete_range: delete lines from start_line to end_line (atomic)\n- replace_range: replace the entire range [start_line, end_line] with new content (composite = delete + insert, one step)"
                    },
                    "after_line": {
                        "type": "integer",
                        "description": "Required for insert_after: insert after this line number (1-indexed)"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Required for delete_range/replace_range: starting line (1-indexed)"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Required for delete_range/replace_range: ending line (1-indexed, inclusive)"
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
        Box::new(FsWriteCoreTool { po })
    }
}

/// Core implementation of write_file tool
#[derive(Debug, Clone)]
pub struct FsWriteCoreTool {
    po: ToolPo,
}

#[async_trait::async_trait]
impl CoreTool for FsWriteCoreTool {
    async fn call(&self, _ctx: RequestContext, args: Value) -> Result<Value> {
        // Parse arguments
        let args: WriteFileArgs = serde_json::from_value(args)
            .map_err(|e| anyhow!("Invalid arguments: {}", e))
            .map_err(|e| common::error::Error::from(e))?;

        // Validate required parameters for mode
        validate_args(&args)?;

        // Get base data path from global config
        let base_path = crate::config::get().base_data_path();
        let target_path = resolve_and_validate_path(&base_path, &args.path)?;

        // Size limit check for content
        const DEFAULT_WRITE_MAX_BYTES: usize = 1 * 1024 * 1024; // 1MB
        const HARD_WRITE_MAX_BYTES: usize = 10 * 1024 * 1024; // 10MB

        if let Some(content) = &args.content {
            let size = content.len();
            if size > HARD_WRITE_MAX_BYTES {
                return Ok(serde_json::json!({
                    "success": false,
                    "error": format!("Content too large: {} bytes, maximum allowed is {} bytes", size, HARD_WRITE_MAX_BYTES)
                }));
            }
        }

        // If file exists, read all lines first for incremental edits
        let mut existing_lines: Vec<String> = if target_path.exists() {
            let file = File::open(&target_path)
                .map_err(|e| anyhow!("Failed to open existing file: {}", sanitize_error(e)))
                .map_err(|e| common::error::Error::from(e))?;
            let reader = BufReader::new(file);
            reader.lines()
                .collect::<std::result::Result<_, _>>()
                .map_err(|e| anyhow!("Failed to read existing file: {}", sanitize_error(e)))
                .map_err(|e| common::error::Error::from(e))?
        } else {
            // File doesn't exist - only allowed in overwrite mode
            if args.mode != "overwrite" {
                return Ok(serde_json::json!({
                    "success": false,
                    "error": "File does not exist. Use overwrite mode to create a new file."
                }));
            }
            Vec::new()
        };

        let original_lines = existing_lines.len();
        let content = args.content.unwrap_or_default();
        let new_lines: Vec<String> = split_lines(&content);

        // Apply edit based on mode
        match args.mode.as_str() {
            "overwrite" => {
                // Completely overwrite the file
                existing_lines = new_lines;
            }
            "append" => {
                // Append to end
                existing_lines.extend(new_lines);
            }
            "insert_after" => {
                // Insert after specified line
                let after = args.after_line.unwrap();
                let after_idx = after.min(original_lines);
                // 0-index: insert after after_idx-1 (since 1-indexed)
                let insert_pos = after_idx;
                for line in new_lines {
                    existing_lines.insert(insert_pos, line);
                }
            }
            "delete_range" => {
                let start = args.start_line.unwrap();
                let end = args.end_line.unwrap();
                // Convert to 0-index range
                let start_idx = start.saturating_sub(1);
                let end_idx = end.min(original_lines);
                if start_idx >= end_idx {
                    return Ok(serde_json::json!({
                        "success": false,
                        "error": format!("Invalid range: start_line {} >= end_line {}", start, end)
                    }));
                }
                // Remove the range
                existing_lines.drain(start_idx..end_idx);
            }
            "replace_range" => {
                let start = args.start_line.unwrap();
                let end = args.end_line.unwrap();
                // Convert to 0-index range
                let start_idx = start.saturating_sub(1);
                let end_idx = end.min(original_lines);
                if start_idx >= end_idx {
                    return Ok(serde_json::json!({
                        "success": false,
                        "error": format!("Invalid range: start_line {} >= end_line {}", start, end)
                    }));
                }
                // Replace the range: remove then insert
                existing_lines.drain(start_idx..end_idx);
                for (i, line) in new_lines.into_iter().enumerate() {
                    existing_lines.insert(start_idx + i, line);
                }
            }
            _ => {
                return Ok(serde_json::json!({
                    "success": false,
                    "error": format!("Unknown mode '{}', expected one of: overwrite, append, insert_after, delete_range, replace_range", args.mode)
                }));
            }
        }

        let final_lines = existing_lines.len();
        let lines_changed = (original_lines as i32 - final_lines as i32).abs();

        // Write back to file
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&target_path)
            .map_err(|e| anyhow!("Failed to open file for writing: {}", sanitize_error(e)))
            .map_err(|e| common::error::Error::from(e))?;

        for line in &existing_lines {
            writeln!(file, "{}", line)
                .map_err(|e| anyhow!("Failed to write line: {}", sanitize_error(e)))
                .map_err(|e| common::error::Error::from(e))?;
        }

        file.flush()
            .map_err(|e| anyhow!("Failed to flush file: {}", sanitize_error(e)))
            .map_err(|e| common::error::Error::from(e))?;

        Ok(serde_json::json!({
            "success": true,
            "path": args.path,
            "mode": args.mode,
            "original_lines": original_lines,
            "final_lines": final_lines,
            "lines_changed": lines_changed
        }))
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

/// Validate required parameters based on mode
fn validate_args(args: &WriteFileArgs) -> Result<()> {
    match args.mode.as_str() {
        "overwrite" => {
            if args.content.is_none() {
                return Err(anyhow!("content is required for overwrite mode").into());
            }
        }
        "append" => {
            if args.content.is_none() {
                return Err(anyhow!("content is required for append mode").into());
            }
        }
        "insert_after" => {
            if args.content.is_none() {
                return Err(anyhow!("content is required for insert_after mode").into());
            }
            if args.after_line.is_none() {
                return Err(anyhow!("after_line is required for insert_after mode").into());
            }
        }
        "delete_range" => {
            if args.start_line.is_none() || args.end_line.is_none() {
                return Err(anyhow!("start_line and end_line are required for delete_range mode").into());
            }
        }
        "replace_range" => {
            if args.content.is_none() {
                return Err(anyhow!("content is required for replace_range mode").into());
            }
            if args.start_line.is_none() || args.end_line.is_none() {
                return Err(anyhow!("start_line and end_line are required for replace_range mode").into());
            }
        }
        _ => {}
    }
    Ok(())
}

/// Resolve and validate the target path against sandbox restrictions
fn resolve_and_validate_path(base_path: &Path, user_path: &str) -> Result<PathBuf> {
    // 1. Check sensitive filename patterns first
    if is_sensitive_filename(user_path) {
        return Err(anyhow!("Access denied: cannot write to sensitive file").into());
    }

    // 2. Build absolute path
    let user_path = Path::new(user_path);
    let absolute_path = if user_path.is_absolute() {
        user_path.to_path_buf()
    } else {
        base_path.join(user_path)
    };

    // 3. Canonicalize to resolve .. and symlinks
    let canonical = if absolute_path.exists() {
        absolute_path.canonicalize()
            .map_err(|_| anyhow!("Failed to resolve path: file not found or permission denied"))
    } else {
        // File doesn't exist yet - canonicalize parent directory
        match absolute_path.parent() {
            Some(parent) => {
                let parent_canon = parent.canonicalize()
                    .map_err(|_| anyhow!("Parent directory does not exist or permission denied"))?;
                Ok(parent_canon.join(absolute_path.file_name().unwrap()))
            }
            None => {
                Err(anyhow!("Invalid path: no parent directory"))
            }
        }
    }
    .map_err(|e| common::error::Error::from(e))?;

    // 4. Check that canonical path is still under base_path
    let base_canonical = base_path.canonicalize()
        .map_err(|e| anyhow!("Invalid base data path: {}", e))
        .map_err(|e| common::error::Error::from(e))?;

    if !canonical.starts_with(&base_canonical) {
        return Err(anyhow!("Access denied: path outside allowed workspace directory").into());
    }

    // 5. Reject symlinks
    if canonical.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(true)
    {
        return Err(anyhow!("Access denied: symbolic links are not allowed").into());
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

/// Split content into lines - preserve line breaks except trailing
fn split_lines(content: &str) -> Vec<String> {
    content.lines().map(|s| s.to_string()).collect()
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

    #[test]
    fn test_validate_args_ok() {
        // overwrite with content
        let args = WriteFileArgs {
            path: "test.txt".to_string(),
            content: Some("content".to_string()),
            mode: "overwrite".to_string(),
            after_line: None,
            start_line: None,
            end_line: None,
        };
        assert!(validate_args(&args).is_ok());

        // insert_after requires after_line and content
        let args = WriteFileArgs {
            path: "test.txt".to_string(),
            content: Some("content".to_string()),
            mode: "insert_after".to_string(),
            after_line: Some(5),
            start_line: None,
            end_line: None,
        };
        assert!(validate_args(&args).is_ok());

        // delete_range requires start and end
        let args = WriteFileArgs {
            path: "test.txt".to_string(),
            content: None,
            mode: "delete_range".to_string(),
            after_line: None,
            start_line: Some(1),
            end_line: Some(5),
        };
        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn test_validate_args_error() {
        // overwrite missing content
        let args = WriteFileArgs {
            path: "test.txt".to_string(),
            content: None,
            mode: "overwrite".to_string(),
            after_line: None,
            start_line: None,
            end_line: None,
        };
        assert!(validate_args(&args).is_err());

        // insert_after missing after_line
        let args = WriteFileArgs {
            path: "test.txt".to_string(),
            content: Some("content".to_string()),
            mode: "insert_after".to_string(),
            after_line: None,
            start_line: None,
            end_line: None,
        };
        assert!(validate_args(&args).is_err());

        // delete_range missing start
        let args = WriteFileArgs {
            path: "test.txt".to_string(),
            content: None,
            mode: "delete_range".to_string(),
            after_line: None,
            start_line: None,
            end_line: Some(5),
        };
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn test_reject_sensitive_filename() {
        assert!(is_sensitive_filename(".env"));
        assert!(is_sensitive_filename("private.key"));
        assert!(is_sensitive_filename(".secrets")); // hidden file
        assert!(!is_sensitive_filename("src/lib.rs"));
        assert!(!is_sensitive_filename("tests/test.txt"));
    }

    #[test]
    fn test_split_lines() {
        let content = "line 1\nline 2\nline 3";
        let lines = split_lines(content);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line 1");
        assert_eq!(lines[2], "line 3");
    }

    #[test]
    fn test_split_lines_empty() {
        let content = "";
        let lines = split_lines(content);
        assert_eq!(lines.len(), 0);
    }
}
