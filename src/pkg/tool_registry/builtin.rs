//! Builtin tool factory - built-in tools are created from constant definitions

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::tool_registry::fs_read::FsReadToolFactory;
use crate::pkg::tool_registry::fs_write::FsWriteToolFactory;
use crate::pkg::tool_registry::http_fetch::HttpFetchToolFactory;
use crate::pkg::tool_registry::shell_exec::ShellExecToolFactory;
use crate::pkg::tool_registry::ToolRegistry;
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
}

clone_trait_object!(BuiltinToolFactory);

/// List of all generic built-in tools (lazy initialized because default needs non-const call)
pub static GENERIC_BUILTIN_TOOLS: Lazy<Vec<(String, Box<dyn BuiltinToolFactory>)>> =
    Lazy::new(|| vec![
        ("http_fetch".to_string(), Box::new(HttpFetchToolFactory::default())),
        ("fs_read".to_string(), Box::new(FsReadToolFactory::default())),
        ("fs_write".to_string(), Box::new(FsWriteToolFactory::default())),
        ("shell_exec".to_string(), Box::new(ShellExecToolFactory::default())),
    ]);

/// Register all generic built-in tools to the global registry
pub fn register_all(registry: &ToolRegistry) {
    for (id, factory) in GENERIC_BUILTIN_TOOLS.iter() {
        let po = factory.create_po();
        registry.register_builtin_factory(factory.clone());
    }
}