//! Global tool registry - each protocol has its own typed storage

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;
use async_trait::async_trait;
use dyn_clone::DynClone;
use dyn_clone::clone_trait_object;

pub mod builtin;
pub mod http;
pub mod mcp;

pub use builtin::BuiltinTool;

/// Object-safe, unified tool interface with JSON I/O.
/// All tools from all protocols eventually get converted to this for calling.
#[async_trait]
pub trait ErasedTool: Send + Sync + DynClone {
    /// Get tool name
    fn name(&self) -> String;
    /// Get OpenAI-style tool definition
    async fn definition(&self, prompt: String) -> rig::completion::ToolDefinition;
    /// Call tool with JSON arguments
    async fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, rig::tool::ToolError>;
}

clone_trait_object!(ErasedTool);

/// Type alias for the dynamic tool we return from registry.
pub type DynTool = Box<dyn ErasedTool>;

/// Global tool registry instance.
pub static GLOBAL_TOOL_REGISTRY: OnceLock<ToolRegistry> = OnceLock::new();

/// Initialize global tool registry.
pub fn init() {
    GLOBAL_TOOL_REGISTRY.set(ToolRegistry::default()).ok();
}

/// Get the global tool registry.
pub fn get_registry() -> &'static ToolRegistry {
    GLOBAL_TOOL_REGISTRY.get().unwrap()
}

/// Global tool registry.
/// Each protocol type has its own typed storage field for better type safety.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    /// Built-in (pre-compiled) tools - stored as their own trait type `Box<dyn BuiltinTool>`
    builtins: Arc<Mutex<HashMap<Uuid, Box<dyn BuiltinTool>>>>,
    /// HTTP remote tools - placeholder for future implementation
    http: Arc<Mutex<HashMap<Uuid, ()>>>,
    /// MCP protocol tools - placeholder for future implementation
    mcp: Arc<Mutex<HashMap<Uuid, ()>>>,
}

impl ToolRegistry {
    /// Register a built-in tool.
    pub fn register_builtin(&self, tool: Box<dyn BuiltinTool>) {
        let id = tool.id();
        self.builtins.lock().unwrap().insert(id, tool);
    }

    /// Get a tool by ID from any registry.
    /// Returns a type-erased DynTool for unified calling.
    pub fn get(&self, id: &Uuid) -> Option<DynTool> {
        let lock = self.builtins.lock().unwrap();
        // BuiltinTool inherits ErasedTool, so we can just return it directly
        lock.get(id).map(|tool| tool.clone() as DynTool)
    }

    /// Unregister a tool by ID from all registries.
    pub fn unregister(&self, id: &Uuid) {
        self.builtins.lock().unwrap().remove(id);
        self.http.lock().unwrap().remove(id);
        self.mcp.lock().unwrap().remove(id);
    }

    /// Clear all registered tools.
    pub fn clear_all(&self) {
        self.builtins.lock().unwrap().clear();
        self.http.lock().unwrap().clear();
        self.mcp.lock().unwrap().clear();
    }

    /// List all registered built-in tool IDs.
    pub fn list_builtin_ids(&self) -> Vec<Uuid> {
        self.builtins.lock().unwrap().keys().cloned().collect()
    }

    /// Get a built-in tool directly (if you need builtin-specific operations).
    pub fn get_builtin(&self, id: &Uuid) -> Option<Box<dyn BuiltinTool>> {
        let lock = self.builtins.lock().unwrap();
        lock.get(id).cloned()
    }
}
