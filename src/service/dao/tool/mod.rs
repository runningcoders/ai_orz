//! Tool DAO trait

use crate::models::tool::{Tool, ToolPo};
use crate::pkg::request_context::RequestContext;
use anyhow::Result;
use async_trait::async_trait;
use std::boxed::Box;

pub mod sqlite;
#[cfg(test)]
mod sqlite_test;

pub use sqlite::{init, dao, new};

/// Tool DAO trait
#[async_trait]
pub trait ToolDao: Send + Sync {
    /// Create a new tool
    async fn create_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<()>;

    /// Update an existing tool
    async fn update_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<()>;

    /// Get tool by ID (only PO)
    async fn get_by_id(&self, ctx: &RequestContext, id: String) -> Result<Option<ToolPo>>;

    /// Get full tool by ID (PO + built ToolDyn from registry)
    async fn get_tool_full(&self, ctx: &RequestContext, id: String) -> Result<Option<Tool>>;

    /// Get tool by name
    async fn get_by_name(&self, ctx: &RequestContext, name: &str) -> Result<Option<ToolPo>>;

    /// List all enabled tools
    async fn list_enabled(&self, ctx: &RequestContext) -> Result<Vec<ToolPo>>;

    /// List full tools for an agent (each is PO + ToolDyn)
    async fn list_tools_for_agent_full(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>>;

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

    /// List all tools for an agent (only POs)
    async fn list_tools_for_agent(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<ToolPo>>;

    /// Sync all registered built-in tools to database
    /// If a tool already exists (by ID), skip it to avoid duplicates
    /// Returns number of newly inserted tools
    async fn sync_builtin_tools_to_db(&self, ctx: &RequestContext) -> Result<usize>;
}
