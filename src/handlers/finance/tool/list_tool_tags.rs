//! Handler: GET /api/v1/finance/tools/tags - 列出所有启用工具的 distinct tags
//!
//! 用于前端工具包安装下拉框数据源

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListToolTagsRequest, ListToolTagsResponse};
use common::error::Result;

/// 列出所有启用工具（status=Enabled）的不重复 tag 列表（按字母升序）
#[register_handler_tool(
    id = "list_tool_tags",
    name = "list_tool_tags",
    description = "List all distinct tags from enabled tools. Useful for discovering available tool categories/packs.",
    params = "common::api::ListToolTagsRequest",
    tags = "tool_management",
    neural
)]
#[generate_http_handler]
pub async fn list_tool_tags(
    ctx: RequestContext,
    _params: ListToolTagsRequest,
) -> Result<ListToolTagsResponse> {
    let tags = domain().tool_provider_manage().list_tool_tags(ctx).await?;
    Ok(ListToolTagsResponse { tags })
}
