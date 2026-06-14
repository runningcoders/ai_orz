//! 列出 Tool

use axum::{
    Json,
    extract::{Extension, Query},
};
use common::api::{ApiResponse, ToolListItem, ToolListQuery};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::dao::tool::ToolQuery;
use crate::service::domain::finance::domain;

use super::response::to_list_item;

/// 列出 Tool
/// GET /tools
pub async fn list_tools(
    Extension(ctx): Extension<RequestContext>,
    Query(req): Query<ToolListQuery>,
) -> Result<Json<ApiResponse<Vec<ToolListItem>>>, AppError> {
    let tools = domain()
        .tool_provider_manage()
        .query_tools(
            ctx,
            ToolQuery {
                agent_id: req.agent_id.clone(),
                keyword: req.keyword.clone(),
                enabled_only: req.enabled_only,
                limit: req.limit,
                ..Default::default()
            },
        )
        .await?;

    let responses = tools.iter().map(to_list_item).collect();
    Ok(Json(ApiResponse::success(responses)))
}
