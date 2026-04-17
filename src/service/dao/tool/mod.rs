//! Tool DAO trait

use crate::models::tool::ToolPo;
use crate::pkg::request_context::RequestContext;
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;
use std::sync::OnceLock;

mod sqlite;

use crate::pkg::tool_registry::{ToolRegistry, GLOBAL_TOOL_REGISTRY, DynTool};

pub use sqlite::{init, SqliteToolDao};

/// Global Tool DAO instance
pub static TOOL_DAO: OnceLock<Box<dyn ToolDao>> = OnceLock::new();

/// Get global Tool DAO
pub fn get() -> &'static Box<dyn ToolDao> {
    TOOL_DAO.get().unwrap()
}

/// Tool DAO trait
#[async_trait]
pub trait ToolDao: Send + Sync {
    /// Create a new tool
    async fn create_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<()>;

    /// Update an existing tool
    async fn update_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<()>;

    /// Get tool by ID
    async fn get_by_id(&self, ctx: &RequestContext, id: Uuid) -> Result<Option<ToolPo>>;

    /// Get tool by name
    async fn get_by_name(&self, ctx: &RequestContext, name: &str) -> Result<Option<ToolPo>>;

    /// List all enabled tools
    async fn list_enabled(&self, ctx: &RequestContext) -> Result<Vec<ToolPo>>;

    /// Add tool to agent
    async fn add_tool_to_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: Uuid,
        created_by: Option<String>,
    ) -> Result<()>;

    /// Remove tool from agent
    async fn remove_tool_from_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: Uuid,
    ) -> Result<()>;

    /// List all tools for an agent
    async fn list_tools_for_agent(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<ToolPo>>;

    // ========== Tool instance operations ==========

    /// Get global tool registry
    fn registry(&self) -> &ToolRegistry {
        GLOBAL_TOOL_REGISTRY.get().unwrap()
    }

    /// Get tool instance by ID from registry
    fn get_tool_instance(&self, id: Uuid) -> Option<DynTool> {
        self.registry().get(&id)
    }
}
