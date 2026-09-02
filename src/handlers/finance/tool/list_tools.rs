//! Handler: GET /api/v1/tools - List tools with filtering (by agent, keyword, enabled status)

use crate::pkg::RequestContext;
use crate::service::dao::tool::ToolQuery;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListToolsRequest, PagedResult, ToolListItem};
use common::error::Result;

use super::response::{probe_runtime_ready, to_list_item};

/// List all tools with optional filtering by agent, keyword, and enabled status
#[register_handler_tool(
    id = "list_tools",
    name = "List All Tools",
    description = "Browse all tools with pagination, returning each tool's basic info and runtime-ready flag. Use for an overview with no specific criteria; use query_tools for structured filtering or search_tools for free-text semantic search.",
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
            ctx.clone(),
            ToolQuery {
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    let ready = probe_runtime_ready(&ctx, &page.items).await;
    Ok(page.map(|t| {
        let runtime_ready = ready.get(&t.po.id).cloned().unwrap_or_default();
        to_list_item(&t, runtime_ready)
    }))
}
