//! MCP Server 子模块实现
//!
//! MCP Server 属于外部能力 Provider 配置，由 Finance Domain 统一管理。

use crate::models::mcp_server::{
    McpServer, McpServerConfig, McpServerQuery, McpServerStatus, REDACTED_CONFIG_VALUE,
};
use crate::pkg::RequestContext;
use crate::service::domain::finance::{FinanceDomainImpl, McpServerManage};
use common::error::Result;

#[async_trait::async_trait]
impl McpServerManage for FinanceDomainImpl {
    /// 创建 MCP Server
    async fn create_mcp_server(&self, ctx: RequestContext, server: &McpServer) -> Result<()> {
        self.mcp_server_dal.create(ctx, server).await
    }

    /// 获取 MCP Server
    async fn get_mcp_server(&self, ctx: RequestContext, id: &str) -> Result<Option<McpServer>> {
        Ok(self
            .mcp_server_dal
            .find_by_id(ctx, id)
            .await?
            .map(McpServer::redacted_for_management))
    }

    /// 通用综合查询
    async fn query_mcp_servers(
        &self,
        ctx: RequestContext,
        query: McpServerQuery,
    ) -> Result<common::api::PagedResult<McpServer>> {
        Ok(self
            .mcp_server_dal
            .query(ctx, query)
            .await?
            .map(McpServer::redacted_for_management))
    }

    /// 列出所有 MCP Server
    async fn list_mcp_servers(&self, ctx: RequestContext) -> Result<Vec<McpServer>> {
        Ok(self
            .mcp_server_dal
            .query(ctx, McpServerQuery::default())
            .await?
            .items
            .into_iter()
            .map(McpServer::redacted_for_management)
            .collect())
    }

    /// 更新 MCP Server
    async fn update_mcp_server(&self, ctx: RequestContext, server: &McpServer) -> Result<()> {
        let mut server_to_update = server.clone();
        if let Some(existing) = self
            .mcp_server_dal
            .find_by_id(ctx.clone(), &server.po.id)
            .await?
        {
            let merged_config = merge_redacted_config(server.po.config(), existing.po.config());
            server_to_update.po.set_config(&merged_config);
        }
        self.mcp_server_dal.update(ctx, &server_to_update).await
    }

    /// 更新 MCP Server 状态
    async fn update_mcp_server_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: McpServerStatus,
    ) -> Result<()> {
        self.mcp_server_dal.set_status(ctx, id, status).await
    }

    /// 删除 MCP Server
    async fn delete_mcp_server(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.mcp_server_dal.delete(ctx, id).await
    }
}

fn merge_redacted_config(
    mut incoming: McpServerConfig,
    existing: McpServerConfig,
) -> McpServerConfig {
    for (key, value) in incoming.env.iter_mut() {
        if value == REDACTED_CONFIG_VALUE {
            if let Some(existing_value) = existing.env.get(key) {
                *value = existing_value.clone();
            }
        }
    }

    for (key, value) in incoming.headers.iter_mut() {
        if value == REDACTED_CONFIG_VALUE {
            if let Some(existing_value) = existing.headers.get(key) {
                *value = existing_value.clone();
            }
        }
    }

    if incoming
        .url
        .as_deref()
        .is_some_and(|url| url.contains(REDACTED_CONFIG_VALUE))
    {
        incoming.url = existing.url;
    }

    incoming
}
