//! Global tool registry - each protocol has its own typed storage

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use dyn_clone;

pub mod builtin;
pub mod http;
pub mod mcp;

use crate::models::tool::{Tool, ToolPo};
pub use builtin::BuiltinToolFactory;

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
/// 
/// Stores FACTORIES, not instances. Instances are created per request from ToolPo
/// loaded from database. This allows user configuration (name/description) in DB
/// to be injected into the tool instance at creation time.
/// 
/// Each protocol type has its own typed storage field for better type safety.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    /// Built-in (pre-compiled) tools - stored as factories that create instances from ToolPo
    builtin_factories: Arc<Mutex<HashMap<String, Box<dyn BuiltinToolFactory>>>>,
    /// Dynamic MCP tools (future) - will store as factories
    mcp_factories: Arc<Mutex<HashMap<String, ()>>>,
    /// Dynamic HTTP tools (future) - will store as factories
    http_factories: Arc<Mutex<HashMap<String, ()>>>,
}

impl ToolRegistry {
    /// Register a built-in tool factory.
    pub fn register_builtin_factory(&self, factory: Box<dyn BuiltinToolFactory>) {
        let id = factory.id().to_string();
        self.builtin_factories.lock().unwrap().insert(id, factory);
    }

    /// Create a tool instance from registry given ToolPo loaded from DB.
    /// 
    /// Dispatches to the correct factory based on protocol type.
    pub fn create_tool(&self, po: ToolPo) -> Option<Box<dyn Tool>> {
        match po.protocol {
            common::enums::ToolProtocol::Builtin => {
                // Lookup factory by id
                let lock = self.builtin_factories.lock().unwrap();
                let factory = lock.get(&po.id)?;
                Some(factory.create(po))
            }
            common::enums::ToolProtocol::Mcp => {
                // Future: create from MCP factory
                None
            }
            common::enums::ToolProtocol::Http => {
                // Future: create from HTTP factory
                None
            }
        }
    }

    /// Get a built-in factory directly.
    pub fn get_builtin_factory(&self, id: &str) -> Option<Box<dyn BuiltinToolFactory>> {
        let lock = self.builtin_factories.lock().unwrap();
        lock.get(id).map(|f| dyn_clone::clone_box(&**f))
    }

    /// Unregister a factory by ID from all registries.
    pub fn unregister(&self, id: &str) {
        self.builtin_factories.lock().unwrap().remove(id);
        self.http_factories.lock().unwrap().remove(id);
        self.mcp_factories.lock().unwrap().remove(id);
    }

    /// Clear all registered factories.
    pub fn clear_all(&self) {
        self.builtin_factories.lock().unwrap().clear();
        self.http_factories.lock().unwrap().clear();
        self.mcp_factories.lock().unwrap().clear();
    }

    /// List all registered built-in tool IDs.
    pub fn list_builtin_ids(&self) -> Vec<String> {
        self.builtin_factories.lock().unwrap().keys().cloned().collect()
    }
}
