//! Handler: GET /api/v1/tasks/{id} - Get task detailed information

use super::response;
use crate::pkg::RequestContext;
use crate::service::dal::task::TaskFetchOptions;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetTaskRequest, GetTaskResponse};
use common::error::Result;
use common::models::StatsInterval;

/// Get task detailed information by ID
#[register_handler_tool(
    id = "get_task",
    name = "Get Task Details",
    description = "Get a task's full detail by ID, optionally loading event stats, model-call stats (hourly or daily within a time range), and the artifact list. Returns the task detail including status, progress, assignee, and dependencies. Fails with not found if the ID does not exist.",
    params = "common::api::GetTaskRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn get_task(ctx: RequestContext, params: GetTaskRequest) -> Result<GetTaskResponse> {
    let options = TaskFetchOptions {
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
        with_artifacts: params.with_artifacts,
    };

    let task = domain()
        .task_manage()
        .get_task(ctx, &params.id, options)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Task {} not found", params.id)))?;

    Ok(response::to_detail(&task))
}
