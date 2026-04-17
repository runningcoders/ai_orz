//! Global tool registry - separate storage by protocol

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

pub mod builtin;
pub mod http;
pub mod mcp;

pub use builtin::DynTool;

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

/// Global tool registry - separate storage by protocol
#[derive(Clone, Default)]
pub struct ToolRegistry {
    /// Built-in tools (pre-compiled in code)
    builtins: Arc<Mutex<HashMap<Uuid, DynTool>>>,
    /// HTTP remote tools
    http: Arc<Mutex<HashMap<Uuid, DynTool>>>,
    /// MCP protocol tools
    mcp: Arc<Mutex<HashMap<Uuid, DynTool>>>,
}

impl ToolRegistry {
    /// Register a built-in tool
    pub fn register_builtin(&self, id: Uuid, tool: DynTool) {
        let mut lock = self.builtins.lock().unwrap();
        lock.insert(id, tool);
    }

    /// Register an HTTP tool
    pub fn register_http(&self, id: Uuid, tool: DynTool) {
        let mut lock = self.http.lock().unwrap();
        lock.insert(id, tool);
    }

    /// Register an MCP tool
    pub fn register_mcp(&self, id: Uuid, tool: DynTool) {
        let mut lock = self.mcp.lock().unwrap();
        lock.insert(id, tool);
    }

    /// Get a tool by ID - checks all registries
    pub fn get(&self, id: &Uuid) -> Option<DynTool> {
        // Check builtins first
        if let Some(tool) = self.builtins.lock().unwrap().get(id) {
            return Some(tool.clone());
        }
        // Then HTTP
        if let Some(tool) = self.http.lock().unwrap().get(id) {
            return Some(tool.clone());
        }
        // Then MCP
        self.mcp.lock().unwrap().get(id).cloned()
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

    /// List all HTTP tool IDs
    pub fn list_http_ids(&self) -> Vec<Uuid> {
        self.http.lock().unwrap().keys().cloned().collect()
    }

    /// List all MCP tool IDs
    pub fn list_mcp_ids(&self) -> Vec<Uuid> {
        self.mcp.lock().unwrap().keys().cloned().collect()
    }
}
