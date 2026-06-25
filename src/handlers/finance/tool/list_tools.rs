//! Handler: GET /api/v1/tools - List tools with filtering (by agent, keyword, enabled status)

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::tool::ToolQuery;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListToolsRequest, ListToolsResponse, ToolListItem};

use super::response::to_list_item;
use common::bail_err;

/// List all tools with optional filtering by agent, keyword, and enabled status
#[register_handler_tool(
    id = "list_tools",
    name = "list_tools",
    description = "List all tools with optional filtering by agent, keyword, and enabled status",
    params = "common::api::ListToolsRequest"
)]
#[generate_http_handler]
pub async fn list_tools(
    ctx: RequestContext,
    params: ListToolsRequest,
) -> Result<ListToolsResponse> {
    let tools = domain()
        .tool_provider_manage()
        .query_tools(
            ctx,
            ToolQuery {
                agent_id: params.agent_id.clone(),
                keyword: params.keyword.clone(),
                enabled_only: params.only_enabled,
                limit: None,
                ..Default::default()
            },
        )
        .await?;

    let tools: Vec<ToolListItem> = tools.iter().map(to_list_item).collect();
    Ok(ListToolsResponse { tools })
}
