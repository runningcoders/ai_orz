//! Builtin tool factory - built-in tools are created from constant definitions

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::tool_registry::ToolRegistry;
use crate::pkg::tool_registry::browser::BrowserToolFactory;
use crate::pkg::tool_registry::doubao_search::DoubaoSearchToolFactory;
use crate::pkg::tool_registry::fs_read::FsReadToolFactory;
use crate::pkg::tool_registry::fs_write::FsWriteToolFactory;
use crate::pkg::tool_registry::gh_cli::GhCliToolFactory;
use crate::pkg::tool_registry::http_fetch::HttpFetchToolFactory;
use crate::pkg::tool_registry::lark_cli::LarkCliToolFactory;
use crate::pkg::tool_registry::mark_artifact::MarkArtifactToolFactory;
use crate::pkg::tool_registry::shell_exec::ShellExecToolFactory;
use crate::pkg::tool_registry::skillhub_cli::SkillhubCliToolFactory;
use crate::pkg::tool_registry::tavily_search::TavilySearchToolFactory;
use dyn_clone::DynClone;
use dyn_clone::clone_trait_object;
use once_cell::sync::Lazy;

/// Built-in tool factory - creates tool instance from given ToolPo
///
/// Each built-in tool registers a factory that knows how to construct itself given the ToolPo from DB.
/// Built-in tools cannot be modified or deleted by users - they can only be synced from code.
pub trait BuiltinToolFactory: DynClone + Send + Sync {
    /// Create default ToolPo for this built-in tool
    fn create_po(&self) -> ToolPo;
    /// Create a tool instance given the ToolPo from DB
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool>;
    /// 工具凭据需求静态声明（默认空；readiness 判定与 call_tool 编排共用，D17）
    ///
    /// 内置工具的需求由代码静态声明（tavily → [TavilyKey]；
    /// doubao → [GenericToken+platform=doubao_search]；
    /// gh/lark 的需求声明在 Step 4 随工厂化落地）。
    fn credential_requirements(&self) -> Vec<common::models::CredentialRequirement> {
        Vec::new()
    }
}

clone_trait_object!(BuiltinToolFactory);

/// List of all generic built-in tools (lazy initialized because default needs non-const call)
pub static GENERIC_BUILTIN_TOOLS: Lazy<Vec<(String, Box<dyn BuiltinToolFactory>)>> =
    Lazy::new(|| {
        vec![
            ("http_fetch".to_string(), Box::new(HttpFetchToolFactory)),
            ("fs_read".to_string(), Box::new(FsReadToolFactory)),
            ("fs_write".to_string(), Box::new(FsWriteToolFactory)),
            ("shell_exec".to_string(), Box::new(ShellExecToolFactory)),
            ("lark_cli".to_string(), Box::new(LarkCliToolFactory)),
            ("gh_cli".to_string(), Box::new(GhCliToolFactory)),
            ("skillhub_cli".to_string(), Box::new(SkillhubCliToolFactory)),
            (
                "tavily_search".to_string(),
                Box::new(TavilySearchToolFactory),
            ),
            (
                "doubao_search".to_string(),
                Box::new(DoubaoSearchToolFactory),
            ),
            ("browser".to_string(), Box::new(BrowserToolFactory)),
            (
                "mark_artifact".to_string(),
                Box::new(MarkArtifactToolFactory),
            ),
        ]
    });

/// Register all generic built-in tools to the global registry
pub fn register_all(registry: &ToolRegistry) {
    for (_id, factory) in GENERIC_BUILTIN_TOOLS.iter() {
        let _po = factory.create_po();
        registry.register_builtin_factory(factory.clone());
    }
}
#[cfg(test)]
#[path = "builtin_tests.rs"]
mod tests;
