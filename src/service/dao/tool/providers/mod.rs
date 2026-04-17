//! Global tool registry - each protocol has its own typed storage

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

pub mod builtin;
pub mod http;
pub mod mcp;

pub use builtin::{BuiltinTool, DynTool, ErasedTool};

/// Global tool registry instance
pub static GLOBAL_TOOL_REGISTRY: OnceLock<ToolRegistry> = OnceLock::new();

/// Initialize global tool registry
pub fn init() {
    GLOBAL_TOOL_REGISTRY.set(ToolRegistry::default()).ok();
}

/// Get global tool registry
pub fn get_registry() -> &'static ToolRegistry {
    GLOBAL_TOOL_REGISTRY.get().unwrap()
}

/// Global tool registry - each protocol has its own typed storage
#[derive(Clone, Default)]
pub struct ToolRegistry {
    /// Built-in tools (pre-compiled in code) - stored as their specific trait type
    builtins: Arc<Mutex<HashMap<Uuid, Box<dyn BuiltinTool>>>>,
    /// HTTP remote tools - will have their own trait type when implemented
    http: Arc<Mutex<HashMap<Uuid, ()>>>,
    /// MCP protocol tools - will have their own trait type when implemented
    mcp: Arc<Mutex<HashMap<Uuid, ()>>>,
}

impl ToolRegistry {
    /// Register a built-in tool
    pub fn register_builtin(&self, tool: Box<dyn BuiltinTool>) {
        let id = tool.id();
        let mut lock = self.builtins.lock().unwrap();
        lock.insert(id, tool);
    }

    /// Register an HTTP tool (placeholder)
    pub fn register_http(&self, _id: Uuid, _tool: ()) {
        let mut lock = self.http.lock().unwrap();
        lock.insert(_id, _tool);
    }

    /// Register an MCP tool (placeholder)
    pub fn register_mcp(&self, _id: Uuid, _tool: ()) {
        let mut lock = self.mcp.lock().unwrap();
        lock.insert(_id, _tool);
    }

    /// Get a tool by ID - checks all registries, returns wrapped DynTool for unified use
    pub fn get(&self, id: &Uuid) -> Option<DynTool> {
        // Check builtins first
        let lock = self.builtins.lock().unwrap();
        if let Some(builtin) = lock.get(id) {
            // Clone the boxed trait object and wrap into DynTool
            return Some(builtin.clone().wrap());
        }
        // TODO: HTTP and MCP when implemented
        None
    }

    /// Unregister a tool from all registries
    pub fn unregister(&self, id: &Uuid) {
        self.builtins.lock().unwrap().remove(id);
        self.http.lock().unwrap().remove(id);
        self.mcp.lock().unwrap().remove(id);
    }

    /// Clear all registries
    pub fn clear_all(&self) {
        self.builtins.lock().unwrap().clear();
        self.http.lock().unwrap().clear();
        self.mcp.lock().unwrap().clear();
    }

    /// List all built-in tool IDs
    pub fn list_builtin_ids(&self) -> Vec<Uuid> {
        self.builtins.lock().unwrap().keys().cloned().collect()
    }

    /// Get a cloned built-in tool directly (if needed for builtin-specific operations)
    pub fn get_builtin(&self, id: &Uuid) -> Option<Box<dyn BuiltinTool>> {
        let lock = self.builtins.lock().unwrap();
        lock.get(id).map(|b| b.clone())
    }
}
