//! Tool Provider 子模块实现
//!
//! 工具提供商配置管理 + Agent 工具借用（绑定）关系
//! 注意：这属于财务管理（计费相关）

use crate::models::tool::Tool;
use crate::pkg::RequestContext;
use crate::pkg::tool_registry::http;
use crate::service::domain::finance::{FinanceDomainImpl, ToolProviderManage};
use common::enums::{ControlMode, ToolProtocol};
use common::error::{Result, bail_err, err};

#[async_trait::async_trait]
impl ToolProviderManage for FinanceDomainImpl {
    /// 创建 Tool
    async fn create_tool(&self, ctx: RequestContext, tool: &Tool) -> Result<()> {
        validate_tool_management_policy(tool)?;
        self.tool_dal.create_tool(ctx.clone(), &tool.po).await
    }

    /// 获取 Tool
    async fn get_tool(&self, ctx: RequestContext, tool_id: &str) -> Result<Option<Tool>> {
        self.tool_dal
            .get_by_id(ctx.clone(), tool_id.to_string())
            .await
    }

    /// 获取 Tool（带附带信息选项）
    async fn get_tool_with_options(
        &self,
        ctx: RequestContext,
        tool_id: &str,
        options: crate::service::dal::tool::ToolFetchOptions,
    ) -> Result<Option<Tool>> {
        self.tool_dal.get_tool(ctx.clone(), tool_id, options).await
    }

    /// 通用综合查询
    async fn query_tools(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::tool::ToolQuery,
    ) -> Result<common::api::PagedResult<Tool>> {
        self.tool_dal.query(ctx.clone(), query).await
    }

    /// 列出所有 Tool
    async fn list_tools(&self, ctx: RequestContext) -> Result<Vec<Tool>> {
        self.tool_dal.list_enabled(ctx.clone()).await
    }

    /// 列出所有启用工具的 distinct tags
    async fn list_tool_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
        self.tool_dal.list_tags(ctx.clone()).await
    }

    /// 同步内置工具到 DB（将 builtin tools registry 中的工具写入 DB）
    async fn sync_builtin_tools(&self, ctx: RequestContext) -> Result<usize> {
        self.tool_dal.sync_builtin_tools_to_db(ctx).await
    }

    /// 更新 Tool
    async fn update_tool(&self, ctx: RequestContext, tool: &Tool) -> Result<()> {
        validate_tool_management_policy(tool)?;
        self.tool_dal.update_tool(ctx.clone(), tool).await
    }

    /// 删除 Tool
    async fn delete_tool(&self, ctx: RequestContext, tool: &Tool) -> Result<()> {
        self.tool_dal.delete_tool(ctx.clone(), &tool.po.id).await
    }

    /// ===== 工具借用（绑定）管理 =====
    /// Agent 借用工具（绑定）
    async fn bind_tool_to_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        let created_by = ctx.uid();
        self.tool_dal
            .add_tool_to_agent(ctx.clone(), agent_id, tool_id, Some(created_by))
            .await
    }

    /// Agent 归还工具（解绑）
    async fn unbind_tool_from_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        self.tool_dal
            .remove_tool_from_agent(ctx.clone(), agent_id, tool_id)
            .await
    }

    /// 获取 Agent 借用的所有工具 ID
    async fn get_agent_bound_tool_ids(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<String>> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        let tools = self
            .tool_dal
            .list_tools_for_agent_full(ctx.clone(), agent_id)
            .await?;
        Ok(tools.into_iter().map(|t| t.po.id).collect())
    }

    /// 获取 Agent 借用的所有工具
    async fn list_agent_tools(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Tool>> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        self.tool_dal
            .list_tools_for_agent_full(ctx.clone(), agent_id)
            .await
    }

    /// 搜索工具（向量 + 关键词混合搜索）
    async fn search_tools(
        &self,
        ctx: RequestContext,
        params: crate::service::dao::tool::ToolSearch,
    ) -> Result<common::api::PagedResult<Tool>> {
        self.tool_dal.search(ctx.clone(), params).await
    }
}

fn validate_tool_management_policy(tool: &Tool) -> Result<()> {
    if !matches!(tool.po.protocol, ToolProtocol::Http | ToolProtocol::Mcp) {
        return Ok(());
    }

    if !matches!(tool.po.control_mode, ControlMode::Manual) {
        let tool_type = match tool.po.protocol {
            ToolProtocol::Http => "HTTP Tool",
            ToolProtocol::Mcp => "Mcp Tool",
            _ => "Tool",
        };
        bail_err!(
            InvalidRequest,
            "{} only supports Manual control mode",
            tool_type
        );
    }

    if matches!(tool.po.protocol, ToolProtocol::Http) {
        http::validate_tool_po_config(&tool.po)
            .map_err(|err| err!(InvalidRequest, "{}", err).with_source(err))?;
    }

    Ok(())
}
