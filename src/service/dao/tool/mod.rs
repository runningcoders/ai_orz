//! Tool DAO trait

use crate::models::tool::{ToolPo};
use crate::pkg::request_context::RequestContext;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub mod sqlite;
#[cfg(test)]
mod sqlite_test;

/// Get global Tool DAO (alias for get, consistent with other DAOs)
pub fn dao() -> Arc<dyn ToolDao> {
    sqlite::dao()
}

/// Initialize global Tool DAO
pub fn init() {
    sqlite::init();
}

/// Tool DAO trait
#[async_trait]
pub trait ToolDao: Send + Sync {
    /// Create a new tool
    async fn create_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<()>;

    /// Update an existing tool
    async fn update_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<()>;

    /// Get tool by ID
    async fn get_by_id(&self, ctx: &RequestContext, id: String) -> Result<Option<ToolPo>>;

    /// Get tool by name
    async fn get_by_name(&self, ctx: &RequestContext, name: &str) -> Result<Option<ToolPo>>;

    /// List all enabled tools
    async fn list_enabled(&self, ctx: &RequestContext) -> Result<Vec<ToolPo>>;

    /// Add tool to agent
    async fn add_tool_to_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: &str,
        created_by: Option<String>,
    ) -> Result<()>;

    /// Remove tool from agent
    async fn remove_tool_from_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()>;

    /// List all tools for an agent
    async fn list_tools_for_agent(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<ToolPo>>;

    /// Sync all registered built-in tools to database
    /// If a tool already exists (by ID), skip it to avoid duplicates
    /// Returns number of newly inserted tools
    async fn sync_builtin_tools_to_db(&self, ctx: &RequestContext) -> Result<usize>;
}
