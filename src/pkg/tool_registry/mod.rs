//! Global tool registry - each protocol has its own typed storage

use dyn_clone;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub mod builtin;
pub mod fs_read;
pub mod fs_write;
pub mod handler_adapter;
pub mod http;
pub mod http_fetch;
pub mod lark_cli;
pub mod mcp;
pub mod shell_exec;
pub mod tool_security;

#[cfg(test)]
mod fs_tests;
#[cfg(test)]
mod shell_tests;

use crate::models::tool::{CoreTool, ToolPo};
pub use builtin::BuiltinToolFactory;
pub use handler_adapter::register_handler_tool;
pub use http::{DefaultHttpToolFactory, HttpToolFactory};
pub use mcp::{McpCoreTool, McpToolConfig};

lazy_static! {
    /// Global tool registry instance - initialized automatically on first access.
    pub static ref GLOBAL_TOOL_REGISTRY: ToolRegistry = ToolRegistry::default();
}

/// Get the global tool registry.
pub fn get_registry() -> &'static ToolRegistry {
    &GLOBAL_TOOL_REGISTRY
}

/// Global tool registry.
///
/// Stores FACTORIES, not instances. Instances are created per request from ToolPo
/// loaded from database. This allows user configuration (name/description) in DB
/// to be injected into the tool instance at creation time.
///
/// Each protocol type has its own typed storage field for better type safety.
#[derive(Clone)]
pub struct ToolRegistry {
    /// Built-in (pre-compiled) tools - stored as factories that create instances from ToolPo
    builtin_factories: Arc<Mutex<HashMap<String, Box<dyn BuiltinToolFactory>>>>,
    /// Dynamic MCP tools (future) - will store as factories
    mcp_factories: Arc<Mutex<HashMap<String, ()>>>,
    /// Dynamic HTTP tools are config-driven and use one protocol-level factory.
    http_factory: Arc<dyn HttpToolFactory>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            builtin_factories: Arc::new(Mutex::new(HashMap::new())),
            mcp_factories: Arc::new(Mutex::new(HashMap::new())),
            http_factory: Arc::new(DefaultHttpToolFactory),
        }
    }
}

impl ToolRegistry {
    /// Create a registry with a custom protocol-level HTTP factory.
    pub fn with_http_factory(http_factory: Arc<dyn HttpToolFactory>) -> Self {
        Self {
            http_factory,
            ..Self::default()
        }
    }

    /// Register a built-in tool factory.
    pub fn register_builtin_factory(&self, factory: Box<dyn BuiltinToolFactory>) {
        let id = factory.create_po().id;
        self.builtin_factories.lock().unwrap().insert(id, factory);
    }

    /// Create a tool instance from registry given ToolPo loaded from DB.
    ///
    /// Dispatches to the correct factory based on protocol type.
    pub fn create_tool(&self, po: ToolPo) -> Option<Box<dyn CoreTool>> {
        match po.protocol {
            common::enums::ToolProtocol::Builtin => {
                // Lookup factory by id
                let lock = self.builtin_factories.lock().unwrap();
                let factory = lock.get(&po.id)?;
                Some(factory.create(po))
            }
            common::enums::ToolProtocol::Mcp => {
                // Stage 2: MCP tools are config-driven stubs. Later McpToolDal
                // stages will pass server/runtime deps to a dedicated factory.
                mcp::create_tool(po).ok()
            }
            common::enums::ToolProtocol::Http => {
                // HTTP tools are database-registered; ToolPo.config stores HttpToolConfig.
                self.http_factory.create(po).ok()
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
        self.mcp_factories.lock().unwrap().remove(id);
    }

    /// Clear all registered factories.
    pub fn clear_all(&self) {
        self.builtin_factories.lock().unwrap().clear();
        self.mcp_factories.lock().unwrap().clear();
    }

    /// List all registered built-in tool IDs.
    pub fn list_builtin_ids(&self) -> Vec<String> {
        self.builtin_factories
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }
}
