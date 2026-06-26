//! MCP Tool management implementation for Finance Domain.

use async_trait::async_trait;
use common::api::{ListMcpToolsByServerRequest, PagedResult};

use common::error::Result;
use crate::models::tool::Tool;
use crate::pkg::RequestContext;

use super::{FinanceDomainImpl, McpToolManage};

#[async_trait]
impl McpToolManage for FinanceDomainImpl {
    async fn sync_mcp_tools(
        &self,
        ctx: RequestContext,
        server_id: &str,
    ) -> Result<usize> {
        self.mcp_tool_dal.sync_from_server(ctx, server_id).await
    }

    async fn list_mcp_tools_by_server(
        &self,
        ctx: RequestContext,
        params: ListMcpToolsByServerRequest,
    ) -> Result<PagedResult<Tool>> {
        self.mcp_tool_dal.list_by_server(ctx, params).await
    }
}
