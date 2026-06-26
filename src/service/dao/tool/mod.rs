//! Tool DAO trait

use common::error::{err, bail_err, Error, Result};
use crate::models::tool::ToolPo;
use crate::models::vector::VectorIndexParams;
use crate::pkg::request_context::RequestContext;
use async_trait::async_trait;
use common::enums::{ToolProtocol, ToolStatus};
use std::sync::Arc;

pub mod sqlite;
pub mod vector;

#[cfg(test)]
mod sqlite_test;

/// Get global Tool DAO (alias for get, consistent with other DAOs)
pub fn dao() -> Arc<dyn ToolDao> {
    sqlite::dao()
}

/// Initialize global Tool DAO
pub fn init() {
    sqlite::init();
    vector::init();
}

/// Tool 查询参数
#[derive(Debug, Clone, Default)]
pub struct ToolQuery {
    pub agent_id: Option<String>,
    pub ids: Option<Vec<String>>, // 按 ID 批量查询
    pub keyword: Option<String>,  // 关键词搜索
    pub protocol: Option<ToolProtocol>,
    pub status: Option<ToolStatus>,
    pub exclude_status: Option<ToolStatus>,
    pub mcp_server_id: Option<String>,
    pub enabled_only: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Tool 搜索参数（向量 + 关键词混合搜索）
#[derive(Debug, Clone, Default)]
pub struct ToolSearch {
    pub keyword: Option<String>,
    pub limit: usize,
    pub agent_id: Option<String>,
    pub enabled_only: bool,
}

/// Tool DAO trait
#[async_trait]
pub trait ToolDao: Send + Sync {
    /// Create a new tool
    async fn create_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<()>;

    /// Update an existing tool
    async fn update_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<()>;

    /// Delete a tool
    async fn delete_tool(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// Get tool by ID
    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<ToolPo>>;

    /// Get tool by name
    async fn get_by_name(&self, ctx: RequestContext, name: &str) -> Result<Option<ToolPo>>;

    /// 通用查询
    async fn query(&self, ctx: RequestContext, query: ToolQuery) -> Result<Vec<ToolPo>>;

    /// List all enabled tools
    async fn list_enabled(&self, ctx: RequestContext) -> Result<Vec<ToolPo>>;

    /// Add tool to agent
    async fn add_tool_to_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
        created_by: Option<String>,
    ) -> Result<()>;

    /// Remove tool from agent
    async fn remove_tool_from_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()>;

    /// List all tools for an agent
    async fn list_tools_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<ToolPo>>;

    /// Sync all registered built-in tools to database
    /// If a tool already exists (by ID), skip it to avoid duplicates
    /// Returns number of newly inserted tools
    async fn sync_builtin_tools_to_db(&self, ctx: RequestContext) -> Result<usize>;

    /// 关键词搜索工具（使用 query 方法的关键词查询实现）
    async fn search(&self, ctx: RequestContext, params: ToolSearch) -> Result<Vec<ToolPo>>;
}

// ==================== ToolVectorDao Trait ====================

/// Tool Vector DAO trait - 仅负责工具向量索引的 CRUD，与基础工具数据解耦
#[async_trait]
pub trait ToolVectorDao: Send + Sync {
    /// 插入或更新工具的向量索引
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        tool_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()>;

    /// 纯向量语义搜索，返回完整的向量行数据 + 相似度距离
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<crate::models::vector::VectorSearchHit>>;

    /// 获取指定工具的完整向量行数据（包含元信息）
    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        tool_id: &str,
    ) -> Result<Option<crate::models::vector::VectorRow>>;
}

// ==================== 统一导出 ====================

// 子模块构造函数别名（用于 DAL 层组合）
pub use sqlite::{dao as base_dao, init as init_base, new as new_tool_dao};
pub use vector::{dao as vector_dao, init as init_vector, new as new_tool_vector_dao};
