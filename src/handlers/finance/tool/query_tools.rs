//! Handler: POST /api/v1/finance/tools/query - Tool 通用查询接口
//!
//! 与 list_tools 的区别：list 是列表场景语法糖（GET + query param），
//! query 是完整查询能力（POST + body），支持复杂组合过滤。

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::tool::ToolQuery;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListToolsResponse, ToolListItem, ToolQueryRequest};

use super::response::to_list_item;

/// Tool 通用查询（POST body，支持完整查询能力）
#[register_handler_tool(
    id = "query_tools",
    name = "query_tools",
    description = "Query tools with full filtering support (ids, keyword, agent_id, tags, protocol, etc.)",
    params = "common::api::ToolQueryRequest",
    neural,
)]
#[generate_http_handler]
pub async fn query_tools(
    ctx: RequestContext,
    params: ToolQueryRequest,
) -> Result<ListToolsResponse> {
    let tools = domain()
        .tool_provider_manage()
        .query_tools(
            ctx,
            ToolQuery {
                ids: params.ids,
                keyword: params.keyword,
                agent_id: params.agent_id,
                tags: params.tags,
                protocol: params.protocol,
                status: params.status,
                mcp_server_id: params.mcp_server_id,
                enabled_only: params.enabled_only,
                limit: params.limit,
                offset: params.offset,
                ..Default::default()
            },
        )
        .await?;

    let tools: Vec<ToolListItem> = tools.iter().map(to_list_item).collect();
    Ok(ListToolsResponse { tools })
}
