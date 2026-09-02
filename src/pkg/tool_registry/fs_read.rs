//! Builtin read_file tool implementation

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::tool_security::fs::{
    ValidationResult, crosses_user_boundary, resolve_and_validate_path, sanitize_error,
};
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};

/// File system tool configuration stored in `ToolPo.config`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FsToolConfig {
    /// Additional allowed paths outside the default `base_data_path`.
    /// All paths are anchored to the project root / base data path.
    pub additional_allowed_paths: Option<Vec<String>>,
}

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
            name: "Read File from Workspace".to_string(),
            description: concat!(
                "Read content from a file in the current project/workspace. ",
                "Supports full file read, range read by line numbers, and grep-style pattern matching. ",
                "Returns content with line numbers for easy editing. ",
                "Reads are allowed across the shared base data directory and configured additional paths. ",
                "Paths inside another user's tree return a require_confirmation result instead of content — stop and ask the user for explicit confirmation first."
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
                        "description": "Optional: only return lines matching this regex pattern, with context lines."
                    },
                    "context_lines": {
                        "type": "integer",
                        "description": "Optional: number of context lines before/after each grep match. Default: 2."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            })),
            config: serde_json::json!(FsToolConfig::default()),
            tags: serde_json::to_string(&vec!["fs".to_string()]).unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(FsReadCoreTool::new(po))
    }
}

/// Core implementation of read_file tool
#[derive(Debug, Clone)]
pub struct FsReadCoreTool {
    po: ToolPo,
    config: FsToolConfig,
}

impl FsReadCoreTool {
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
impl CoreTool for FsReadCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // Parse arguments
        let args: ReadFileArgs = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;

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
                // 用户树身份边界：读取其他用户目录需用户确认
                let base_root = crate::config::get().base_data_path();
                if crosses_user_boundary(&base_root, &target_path, ctx.user_id.as_deref()) {
                    return Ok(serde_json::json!({
                        "success": false,
                        "require_confirmation": true,
                        "message": format!(
                            "Path '{}' is inside another user's directory. \
                            You MUST STOP and ask the user for explicit confirmation before accessing it.",
                            args.path
                        )
                    }));
                }

                // Check file size before opening
                let metadata = std::fs::metadata(&target_path)
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to read file metadata: {}", sanitize_error(e))
                    })
                    .map_err(common::error::Error::from)?;

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
                    .map_err(common::error::Error::from)?;
                let reader = BufReader::new(file);
                let lines: Vec<String> = reader
                    .lines()
                    .collect::<std::result::Result<_, _>>()
                    .map_err(|e| anyhow::anyhow!("Failed to read file: {}", sanitize_error(e)))
                    .map_err(common::error::Error::from)?;

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
        }
    }
    fn po(&self) -> &ToolPo {
        &self.po
    }
}

/// Find grep matches with context in the list of lines
fn find_grep_matches(lines: &[String], pattern: &str, context_lines: usize) -> Vec<GrepMatch> {
    let mut matches = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if line.contains(pattern) {
            let line_number = idx + 1; // 1-indexed
            let context_before_start = idx.saturating_sub(context_lines);
            let context_after_end = (idx + context_lines + 1).min(lines.len());

            let context_before = lines[context_before_start..idx].to_vec();
            let context_after = lines[idx + 1..context_after_end].to_vec();

            let content = format!("{:>4}|{}", line_number, line);
            matches.push(GrepMatch {
                line_number,
                content,
                context_before,
                context_after,
            });
        }
    }
    matches
}

/// Resolve start and end line numbers to 0-indexed range
fn resolve_line_range(start: Option<usize>, end: Option<usize>, total: usize) -> (usize, usize) {
    let start_idx = match start {
        Some(s) if s > 0 => s - 1, // convert 1-indexed to 0-indexed
        _ => 0,
    };
    let end_idx = match end {
        Some(e) if e <= total => e,
        _ => total,
    };
    // Clamp to valid range
    (start_idx.min(total), end_idx.max(start_idx))
}
