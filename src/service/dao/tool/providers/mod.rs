//! Global tool instance registry (all protocols)

use anyhow::{anyhow, Result};
use common::enums::ToolProtocol;
use crate::models::tool::ToolPo;
use self::builtin::DynTool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

pub mod builtin;
pub mod http;
pub mod mcp;

/// Global tool registry (key: tool_id)
/// All tools (builtin/http/mcp) are registered here
pub static GLOBAL_TOOL_REGISTRY: OnceLock<ToolRegistry> = OnceLock::new();

/// Initialize global tool registry
pub fn init() {
    GLOBAL_TOOL_REGISTRY.set(ToolRegistry::default()).ok();
}

/// Get global tool registry
pub fn get_registry() -> &'static ToolRegistry {
    GLOBAL_TOOL_REGISTRY.get().unwrap()
}

/// Global tool registry: holds all tool instances by id
#[derive(Clone, Default)]
pub struct ToolRegistry {
    registry: Arc<Mutex<HashMap<Uuid, DynTool>>>,
}

impl ToolRegistry {
    /// Register a tool instance
    pub fn register(&self, id: Uuid, tool: DynTool) {
        let mut lock = self.registry.lock().unwrap();
        lock.insert(id, tool);
    }

    /// Get a tool instance by id
    pub fn get(&self, id: &Uuid) -> Option<DynTool> {
        let lock = self.registry.lock().unwrap();
        lock.get(id).cloned()
    }

    /// Unregister a tool
    pub fn unregister(&self, id: &Uuid) {
        let mut lock = self.registry.lock().unwrap();
        lock.remove(id);
    }

    /// Clear all registered tools
    pub fn clear(&self) {
        let mut lock = self.registry.lock().unwrap();
        lock.clear();
    }
}
