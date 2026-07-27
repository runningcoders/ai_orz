//! Handler: GET /api/v1/projects/{id} - Get project detailed information

use super::response;
use crate::pkg::RequestContext;
use crate::service::dal::project::ProjectFetchOptions;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetProjectRequest, GetProjectResponse};
use common::error::Result;
use common::models::StatsInterval;

/// Get project detailed information
#[register_handler_tool(
    id = "get_project",
    name = "get_project",
    description = "Get project detailed information by ID",
    params = "common::api::GetProjectRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn get_project(
    ctx: RequestContext,
    params: GetProjectRequest,
) -> Result<GetProjectResponse> {
    let options = ProjectFetchOptions {
        with_stats: params.with_stats,
        with_model_call_stats: params.with_model_call_stats,
        stats_time_range: match (params.stats_time_start, params.stats_time_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        },
        stats_interval: params.stats_interval.as_deref().and_then(|s| {
            match s.to_lowercase().as_str() {
                "hourly" => Some(StatsInterval::Hourly),
                "daily" => Some(StatsInterval::Daily),
                _ => None,
            }
        }),
    };

    let project = domain()
        .project_manage()
        .get_project(ctx, &params.id, options)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("Project {} not found", params.id))
        })?;

    Ok(response::to_detail(&project))
}
