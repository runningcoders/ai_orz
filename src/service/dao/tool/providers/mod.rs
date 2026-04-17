//! Tool instance cache and provider dispatcher

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

/// Global tool instance cache (key: tool_id)
pub static GLOBAL_INSTANCE_CACHE: OnceLock<ToolInstanceCache> = OnceLock::new();

/// Global builtin tool registry (key: tool_name)
pub static GLOBAL_BUILTIN_REGISTRY: OnceLock<BuiltinRegistry> = OnceLock::new();

/// Initialize global instance cache and builtin registry
pub fn init_global() {
    GLOBAL_INSTANCE_CACHE.set(ToolInstanceCache::default()).ok();
    GLOBAL_BUILTIN_REGISTRY.set(BuiltinRegistry::new()).ok();
}

/// Builtin tool registry: holds all pre-registered builtin tool instances
pub struct BuiltinRegistry {
    registry: Arc<Mutex<HashMap<String, DynTool>>>,
}

impl BuiltinRegistry {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a builtin tool instance
    pub fn register_raw(&self, name: &str, tool: DynTool) {
        let mut lock = self.registry.lock().unwrap();
        lock.insert(name.to_string(), tool);
    }

    /// Get a registered builtin tool by name
    pub fn get(&self, name: &str) -> Option<DynTool> {
        let lock = self.registry.lock().unwrap();
        lock.get(name).cloned()
    }
}

/// Tool instance cache (key: tool_id)
#[derive(Clone, Default)]
pub struct ToolInstanceCache {
    cache: Arc<Mutex<HashMap<Uuid, Option<DynTool>>>>,
}

impl ToolInstanceCache {
    /// Get or build a Rig Tool instance
    pub fn get_or_build(&self, po: &ToolPo) -> Result<DynTool> {
        let mut guard = self.cache.lock().unwrap();

        if let Some(cached) = guard.get(&po.id) {
            return Ok(cached.clone().unwrap());
        }

        let tool = self.build(po)?;
        guard.insert(po.id, Some(tool.clone()));
        Ok(tool)
    }

    /// Build a new tool instance by protocol
    fn build(&self, po: &ToolPo) -> Result<DynTool> {
        match po.protocol {
            ToolProtocol::Builtin => builtin::build(po),
            ToolProtocol::Http => http::build(po),
            ToolProtocol::Mcp => mcp::build(po),
        }
    }

    /// Invalidate cache for a specific tool
    pub fn invalidate(&self, id: Uuid) {
        self.cache.lock().unwrap().remove(&id);
    }

    /// Clear all cache
    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
    }
}
