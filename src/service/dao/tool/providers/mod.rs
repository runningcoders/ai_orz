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

/// Global tool instance cache
pub static GLOBAL_INSTANCE_CACHE: OnceLock<ToolInstanceCache> = OnceLock::new();

/// Initialize global instance cache
pub fn init_global_cache() {
    GLOBAL_INSTANCE_CACHE.set(ToolInstanceCache::default()).ok();
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
