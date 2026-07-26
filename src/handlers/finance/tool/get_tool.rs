//! Handler: GET /api/v1/tools/{id} - Get tool detailed information

use crate::pkg::RequestContext;
use crate::service::dal::tool::ToolFetchOptions;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetToolRequest, GetToolResponse};
use common::error::Result;
use common::models::StatsInterval;

use super::response::to_detail;

/// Get detailed information about a specific tool including configuration
#[register_handler_tool(
    id = "get_tool",
    name = "get_tool",
    description = "Get detailed information about a specific tool including configuration",
    params = "common::api::GetToolRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn get_tool(ctx: RequestContext, params: GetToolRequest) -> Result<GetToolResponse> {
    // 构建 FetchOptions
    let options = ToolFetchOptions {
        with_stats: params.with_stats,
        stats_time_range: match (params.stats_time_start, params.stats_time_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        },
        stats_interval: params
            .stats_interval
            .and_then(|s| match s.to_lowercase().as_str() {
                "hourly" => Some(StatsInterval::Hourly),
                "daily" => Some(StatsInterval::Daily),
                _ => None,
            }),
    };

    let tool = domain()
        .tool_provider_manage()
        .get_tool_with_options(ctx, &params.id, options)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Tool {} not found", params.id)))?;

    Ok(to_detail(&tool))
}
