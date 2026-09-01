//! Handler: POST /api/v1/tools/search - Search tools with full filtering
//!
//! 与 query_tools 的区别：search 重在"语义相关性"（FTS5 + 向量语义混合搜索），
//! query 重在"条件过滤"。两者现在都支持完整过滤条件和分页返回。

use crate::pkg::RequestContext;
use crate::service::dao::tool::{ToolQuery, ToolSearch};
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, SearchToolsRequest, ToolListItem};
use common::enums::ToolStatus;
use common::error::Result;

use super::response::{probe_runtime_ready, to_list_item};

/// Search tools with full filtering (FTS5 + vector semantic search)
#[register_handler_tool(
    id = "search_tools",
    name = "Search Tools (Semantic)",
    description = "Search tools by keyword with full filtering support (FTS5 + vector semantic search).",
    params = "common::api::SearchToolsRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn search_tools(
    ctx: RequestContext,
    params: SearchToolsRequest,
) -> Result<PagedResult<ToolListItem>> {
    let search = ToolSearch {
        keyword: params.keyword,
        filters: ToolQuery {
            ids: params.ids,
            agent_id: params.agent_id,
            tags: params.tags,
            protocol: params.protocol,
            status: params.status,
            mcp_server_id: params.mcp_server_id,
            enabled_only: params.enabled_only,
            exclude_status: Some(ToolStatus::Stale),
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain()
        .tool_provider_manage()
        .search_tools(ctx.clone(), search)
        .await?;

    let ready = probe_runtime_ready(&ctx, &page.items).await;
    Ok(page.map(|t| {
        let runtime_ready = ready.get(&t.po.id).cloned().unwrap_or_default();
        to_list_item(&t, runtime_ready)
    }))
}
