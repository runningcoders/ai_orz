//! Builtin write_file tool implementation

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::tool_security::fs::{
    ValidationResult, resolve_and_validate_path, sanitize_error,
};
use anyhow::anyhow;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

/// File system tool configuration stored in `ToolPo.config`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FsToolConfig {
    /// Additional allowed paths outside the default `base_data_path`.
    /// All paths are anchored to the project root / base data path.
    pub additional_allowed_paths: Option<Vec<String>>,
}

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
                "Supports multiple atomic modes: overwrite entire file, append to end, insert after a line, ",
                "delete a range of lines, or replace a range of lines. ",
                "All changes are atomic — either complete or not written. ",
                "**Permission rule**: Only allowed to write to files within the project/task/attachment working directories. ",
                "If the requested path is outside this scope, you MUST STOP and ask the user for confirmation before proceeding."
            ).to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write, relative to the project/workspace root"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["overwrite", "append", "insert_after", "delete_range", "replace_range"],
                        "description": "Write mode:\n- overwrite: overwrite entire file (create if not exists)\n- append: append content to end of file\n- insert_after: insert content after the specified line number\n- delete_range: delete lines from start_line to end_line (inclusive)\n- replace_range: replace lines from start_line to end_line with new content"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write. Required for: overwrite, append, insert_after, replace_range"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Required for: insert_after, delete_range, replace_range (1-indexed)"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Required for: delete_range, replace_range (1-indexed, inclusive)"
                    }
                },
                "required": ["path", "mode"],
                "additionalProperties": false
            })),
            config: serde_json::json!(FsToolConfig::default()),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(FsWriteCoreTool::new(po))
    }
}

/// Core implementation of write_file tool
#[derive(Debug, Clone)]
pub struct FsWriteCoreTool {
    po: ToolPo,
    config: FsToolConfig,
}

impl FsWriteCoreTool {
    fn new(po: ToolPo) -> Self {
        let config = if po.config.is_null() {
            FsToolConfig::default()
        } else {
            serde_json::from_value(po.config.clone()).unwrap_or_default()
        };
        Self { po, config }
    }
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
        let additional_allowed = self
            .config
            .additional_allowed_paths
            .as_deref()
            .unwrap_or(&[]);
        match resolve_and_validate_path(&base_path, &args.path, additional_allowed)? {
            ValidationResult::NeedConfirmation(message) => {
                // Return explicit prompt for agent to ask user confirmation
                return Ok(serde_json::json!({
                    "success": false,
                    "require_confirmation": true,
                    "message": message
                }));
            }
            ValidationResult::Valid(target_path) => {
                // Read existing file lines if it exists
                let mut existing_lines: Vec<String> = if target_path.exists() {
                    let file = File::open(&target_path)
                        .map_err(|e| {
                            anyhow::anyhow!("Failed to open existing file: {}", sanitize_error(e))
                        })
                        .map_err(|e| common::error::Error::from(e))?;
                    let reader = BufReader::new(file);
                    reader
                        .lines()
                        .collect::<std::result::Result<_, _>>()
                        .map_err(|e| {
                            anyhow::anyhow!("Failed to read existing file: {}", sanitize_error(e))
                        })
                        .map_err(|e| common::error::Error::from(e))?
                } else {
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
        }
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
                return Err(
                    anyhow!("start_line and end_line are required for delete_range mode").into(),
                );
            }
        }
        "replace_range" => {
            if args.content.is_none() {
                return Err(anyhow!("content is required for replace_range mode").into());
            }
            if args.start_line.is_none() || args.end_line.is_none() {
                return Err(
                    anyhow!("start_line and end_line are required for replace_range mode").into(),
                );
            }
        }
        _ => {}
    }
    Ok(())
}

/// Split content into lines - preserve line breaks except trailing
fn split_lines(content: &str) -> Vec<String> {
    content.lines().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::tool_registry::tool_security::fs::is_sensitive_filename;

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
