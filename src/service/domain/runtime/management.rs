//! Tool 运行时查询子模块
//!
//! 【职责边界】
//! - 只负责运行时工具查询、搜索（Agent 执行时需要）
//! - 不负责工具配置管理（create/update/enable/disable）→ 归 Finance Domain
//! - 不负责工具绑定管理（bind/unbind）→ 归 Finance Domain

use async_trait::async_trait;
use std::fmt::Debug;

use crate::error::AppError;
use crate::models::tool::Tool;
use crate::pkg::request_context::RequestContext;

/// Tool 运行时查询 trait
///
/// 仅提供运行时执行所需的工具查询能力
#[async_trait]
pub trait ToolManagement: Send + Sync + Debug {
    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: &RequestContext,
        query: crate::service::dao::tool::ToolQuery,
    ) -> Result<Vec<Tool>, AppError>;

    /// 获取所有启用的工具列表
    async fn list_tools(&self, ctx: &RequestContext) -> Result<Vec<Tool>, AppError>;

    /// 获取某个 Agent 绑定的所有工具
    async fn list_agent_tools(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>, AppError>;

    /// 根据 ID 获取工具
    async fn get_tool(&self, ctx: &RequestContext, tool_id: &str) -> Result<Option<Tool>, AppError>;

    /// 获取 Agent 绑定的工具 ID 列表
    async fn get_agent_bound_tool_ids(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<String>, AppError>;

    /// 搜索工具（向量 + 关键词混合搜索）
    ///
    /// Agent 思考时选择工具用
    async fn search(&self, ctx: &RequestContext, params: crate::service::dao::tool::ToolSearch) -> Result<Vec<Tool>, AppError>;
}

/// ToolManagement 默认实现
#[derive(Debug, Clone)]
pub struct ToolManagementImpl;

impl ToolManagementImpl {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolManagementImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolManagement for ToolManagementImpl {
    /// 通用综合查询
    async fn query(
        &self,
        ctx: &RequestContext,
        query: crate::service::dao::tool::ToolQuery,
    ) -> Result<Vec<Tool>, AppError> {
        crate::service::dal::tool::dal().query(ctx, query).await
    }

    /// 获取所有启用的工具列表
    async fn list_tools(&self, ctx: &RequestContext) -> Result<Vec<Tool>, AppError> {
        crate::service::dal::tool::dal().list_enabled(ctx).await
    }

    /// 获取某个 Agent 绑定的所有工具
    async fn list_agent_tools(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>, AppError> {
        crate::service::dal::tool::dal().list_tools_for_agent_full(ctx, agent_id).await
    }

    async fn get_tool(&self, ctx: &RequestContext, tool_id: &str) -> Result<Option<Tool>, AppError> {
        crate::service::dal::tool::dal().get_by_id(ctx, tool_id.to_string()).await
    }

    async fn get_agent_bound_tool_ids(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<String>, AppError> {
        let tools = self.list_agent_tools(ctx, agent_id).await?;
        Ok(tools.into_iter().map(|t| t.po.id).collect())
    }

    async fn search(
        &self,
        ctx: &RequestContext,
        params: crate::service::dao::tool::ToolSearch,
    ) -> Result<Vec<Tool>, AppError> {
        crate::service::dal::tool::dal().search(ctx, params).await
    }
}