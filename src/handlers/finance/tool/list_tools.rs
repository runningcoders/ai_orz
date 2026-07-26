//! Handler: GET /api/v1/tools - List tools with filtering (by agent, keyword, enabled status)

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::tool::ToolQuery;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListToolsRequest, PagedResult, ToolListItem};

use super::response::to_list_item;

/// List all tools with optional filtering by agent, keyword, and enabled status
#[register_handler_tool(
    id = "list_tools",
    name = "list_tools",
    description = "List all tools with optional filtering by agent, keyword, and enabled status",
    params = "common::api::ListToolsRequest",
    neural,
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn list_tools(
    ctx: RequestContext,
    params: ListToolsRequest,
) -> Result<PagedResult<ToolListItem>> {
    // list 是语法糖：只接受分页
    let page = domain()
        .tool_provider_manage()
        .query_tools(
            ctx,
            ToolQuery {
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(|t| to_list_item(&t)))
}
