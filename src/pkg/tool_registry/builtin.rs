//! Builtin tool provider

use rig::tool::{Tool, ToolDyn};
use rig::wasm_compat::WasmBoxedFuture;
use uuid::Uuid;
use dyn_clone::DynClone;
use dyn_clone::clone_trait_object;
use rig::tool::ToolError;
use rig::completion::ToolDefinition;

/// Built-in tool trait - adds metadata on top of Rig's ToolDyn.
/// Inherits ToolDyn directly, so any Box<dyn BuiltinTool> is already a Box<dyn ToolDyn>.
pub trait BuiltinTool: ToolDyn + DynClone + Send + Sync {
    /// Unique tool ID (UUID string)
    fn tool_id(&self) -> &'static str;
    /// Tool description for registry/DB
    fn description(&self) -> &'static str;

    /// Get parsed UUID
    fn id(&self) -> Uuid {
        Uuid::parse_str(self.tool_id()).expect("Invalid tool ID UUID")
    }
}

clone_trait_object!(BuiltinTool);

/// Concrete implementation that wraps any Rig Tool + metadata.
#[derive(Clone)]
pub struct BuiltinToolImpl<T> {
    tool: T,
    tool_id: &'static str,
    description: &'static str,
}

impl<T> BuiltinToolImpl<T> {
    pub fn new(tool: T, tool_id: &'static str, description: &'static str) -> Self {
        Self { tool, tool_id, description }
    }
}

impl<T> BuiltinTool for BuiltinToolImpl<T>
where
    T: Tool + Clone + Send + Sync + 'static,
    T::Args: for<'de> serde::Deserialize<'de>,
    T::Output: serde::Serialize,
{
    fn tool_id(&self) -> &'static str {
        self.tool_id
    }

    fn description(&self) -> &'static str {
        self.description
    }
}

// Implement ToolDyn - same as Rig's default implementation for T: Tool
impl<T> ToolDyn for BuiltinToolImpl<T>
where
    T: Tool + Clone + Send + Sync + 'static,
    T::Args: for<'de> serde::Deserialize<'de>,
    T::Output: serde::Serialize,
{
    fn name(&self) -> String {
        self.tool.name()
    }

    fn definition<'a>(&'a self, prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(self.tool.definition(prompt))
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            match serde_json::from_str(&args) {
                Ok(args) => self.tool.call(args)
                    .await
                    .map_err(|e| ToolError::ToolCallError(Box::new(e)))
                    .and_then(|output| {
                        serde_json::to_string(&output).map_err(ToolError::JsonError)
                    }),
                Err(e) => Err(ToolError::JsonError(e)),
            }
        })
    }
}
