//! mark_artifact builtin tool implementation
//!
//! ③ 产物层声明式归档入口：Agent 从工具调用结果（② 层）拿到 call_id，
//! 把对应的运行日志（① 层，tools/{tool_id}/logs/{YYYYMMDD}/{call_id}.log）
//! 复制晋升为项目产物（GeneratedContent，带 tool-output 标记）。
//!
//! 分层约定：pkg 层不感知 Domain —— 归档动作经 `ArtifactRegistrar` trait
//! 注入（service::init 注册 ProjectToolOutputRegistrar），与 browser 截图的
//! ScreenshotStorer 模式一致。

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::paths;
use crate::pkg::request_context::RequestContext;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;

// ==================== 产物注册器（pkg 层抽象）====================

/// 归档成功的产物引用
#[derive(Debug, Clone)]
pub struct ToolOutputArtifact {
    /// 产物 ID（产物中心可见/可下载）
    pub artifact_id: String,
    /// 产物名称
    pub name: String,
}

/// 工具输出产物注册器（pkg 层抽象，由上层 Domain 实现并注册）
#[async_trait::async_trait]
pub trait ArtifactRegistrar: Send + Sync {
    /// 把 call_id 对应的工具运行日志复制晋升为项目产物，返回产物引用
    #[allow(clippy::too_many_arguments)]
    async fn register_tool_output(
        &self,
        ctx: RequestContext,
        call_id: String,
        log_path: PathBuf,
        project_id: String,
        task_id: Option<String>,
        name: String,
        description: String,
    ) -> Result<ToolOutputArtifact>;
}

static ARTIFACT_REGISTRAR: OnceLock<Box<dyn ArtifactRegistrar>> = OnceLock::new();

/// 注册全局产物注册器（service::init 阶段调用，仅首次生效）
pub fn set_artifact_registrar(registrar: Box<dyn ArtifactRegistrar>) {
    let _ = ARTIFACT_REGISTRAR.set(registrar);
}

/// 获取已注册的全局产物注册器
pub fn get_artifact_registrar() -> Option<&'static dyn ArtifactRegistrar> {
    ARTIFACT_REGISTRAR.get().map(|s| s.as_ref())
}

// ==================== 日志定位 ====================

/// 按 call_id 在 `tools/*/logs/*/{call_id}.log` 定位运行日志文件
///
/// 以 call_id 为关联键扫描（不接受路径输入，天然防伪造/路径穿越）；
/// 归档为低频操作，扫描成本可控。
pub fn find_tool_log_by_call_id(
    base_data_path: &std::path::Path,
    call_id: &str,
) -> Option<PathBuf> {
    let tools_dir = paths::tools_root_dir(base_data_path);
    let tool_entries = std::fs::read_dir(&tools_dir).ok()?;
    for tool_entry in tool_entries.flatten() {
        let logs_root = tool_entry.path().join("logs");
        let Ok(day_entries) = std::fs::read_dir(&logs_root) else {
            continue;
        };
        for day_entry in day_entries.flatten() {
            let candidate = day_entry.path().join(format!("{}.log", call_id));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

// ==================== 工具定义 ====================

/// `mark_artifact` 工具参数
#[derive(Debug, Deserialize)]
pub struct MarkArtifactParams {
    /// 要归档的工具调用 ID（来自此前工具结果里的 call_id 字段）
    pub call_id: String,
    /// 归档目标项目（缺省用当前上下文 project_id）
    pub project_id: Option<String>,
    /// 归档目标任务（缺省用当前上下文 task_id）
    pub task_id: Option<String>,
    /// 产物名称（缺省 tool-output-{call_id}）
    pub name: Option<String>,
    /// 产物描述（缺省自动生成）
    pub description: Option<String>,
}

/// mark_artifact 内置工具工厂
#[derive(Debug, Clone, Default)]
pub struct MarkArtifactToolFactory;

impl crate::pkg::tool_registry::BuiltinToolFactory for MarkArtifactToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "mark_artifact".to_string(),
            name: "Archive Tool Output as Artifact".to_string(),
            description: concat!(
                "Archive a previous tool call's full runtime output (log file) as a project ",
                "artifact (deliverable). Use this when a tool's output is worth preserving for ",
                "the user or the project — e.g. a build report, test run, or command output ",
                "that completes a task. The output is copied into the project's artifact ",
                "storage with a tool-output tag; the original log stays under normal retention. ",
                "Provide the call_id from the earlier tool result. Requires a project context ",
                "(explicit project_id or the current conversation's project)."
            )
            .to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "call_id": {
                        "type": "string",
                        "description": "The call_id of the earlier tool call whose output should be archived (found in that tool's result)."
                    },
                    "project_id": {
                        "type": "string",
                        "description": "Optional: target project for the artifact. Defaults to the current project context."
                    },
                    "task_id": {
                        "type": "string",
                        "description": "Optional: target task for the artifact. Defaults to the current task context."
                    },
                    "name": {
                        "type": "string",
                        "description": "Optional: artifact display name. Defaults to tool-output-{call_id}."
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional: artifact description explaining why this output is a deliverable."
                    }
                },
                "required": ["call_id"],
                "additionalProperties": false
            })),
            tags: serde_json::to_string(&vec!["artifact".to_string()]).unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(MarkArtifactCoreTool::new(po))
    }
}

/// mark_artifact 工具实现
#[derive(Debug, Clone)]
pub struct MarkArtifactCoreTool {
    po: ToolPo,
}

impl MarkArtifactCoreTool {
    fn new(po: ToolPo) -> Self {
        Self { po }
    }
}

#[async_trait::async_trait]
impl CoreTool for MarkArtifactCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        let params: MarkArtifactParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid arguments: {}", e))?;

        // 归档目标：显式参数优先，缺省回退当前上下文（对话已绑定项目/任务时零参数可归档）
        let project_id = match params.project_id.or_else(|| ctx.project_id().cloned()) {
            Some(id) => id,
            None => {
                return Ok(serde_json::json!({
                    "success": false,
                    "error": "缺少归档目标项目：请传 project_id，或在绑定了项目的对话中调用"
                }));
            }
        };
        let task_id = params.task_id.or_else(|| ctx.task_id().cloned());
        let name = params
            .name
            .unwrap_or_else(|| format!("tool-output-{}", params.call_id));
        let description = params
            .description
            .unwrap_or_else(|| format!("工具调用 {} 的运行输出归档", params.call_id));

        // 定位 ① 层运行日志（call_id 关联键，不接受路径输入）
        let base_path = crate::config::get().base_data_path();
        let Some(log_path) = find_tool_log_by_call_id(&base_path, &params.call_id) else {
            return Ok(serde_json::json!({
                "success": false,
                "error": format!("未找到 call_id={} 对应的工具运行日志（可能已被保留策略清理或 call_id 无效）", params.call_id)
            }));
        };

        // ③ 层复制晋升（Domain 实现）
        let Some(registrar) = get_artifact_registrar() else {
            return Ok(serde_json::json!({
                "success": false,
                "error": "产物注册器未初始化"
            }));
        };
        let artifact = registrar
            .register_tool_output(
                ctx.clone(),
                params.call_id.clone(),
                log_path.clone(),
                project_id,
                task_id,
                name,
                description,
            )
            .await?;

        Ok(serde_json::json!({
            "success": true,
            "call_id": params.call_id,
            "artifact_id": artifact.artifact_id,
            "name": artifact.name,
            "source_log": log_path.to_string_lossy(),
            "message": "工具输出已归档为项目产物（复制晋升，原日志仍受保留策略管理）"
        }))
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_tool_log_locates_call_id_across_tools_and_days() {
        let base = tempfile::tempdir().unwrap();
        let log_dir = paths::tool_logs_dir(base.path(), "shell_exec").join("20260819");
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("call-abc.log"), "output").unwrap();

        let found = find_tool_log_by_call_id(base.path(), "call-abc");
        assert_eq!(found, Some(log_dir.join("call-abc.log")));
        assert!(find_tool_log_by_call_id(base.path(), "call-missing").is_none());
    }

    #[test]
    fn find_tool_log_rejects_path_like_call_id() {
        let base = tempfile::tempdir().unwrap();
        // call_id 含路径分隔符时拼不出合法文件名（找不到即返回 None），天然防穿越
        assert!(find_tool_log_by_call_id(base.path(), "../etc/passwd").is_none());
    }
}
