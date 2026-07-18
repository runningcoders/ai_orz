//! Handler: GET /api/v1/projects - List all projects for a user with optional filtering

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListProjectsRequest, ProjectListItem};
use common::error::{Result, bail_err};

/// List all projects for a user with optional filtering
#[register_handler_tool(
    id = "list_projects",
    name = "list_projects",
    description = "List all projects for a user with optional status filtering",
    params = "common::api::ListProjectsRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn list_projects(
    ctx: RequestContext,
    params: ListProjectsRequest,
) -> Result<Vec<ProjectListItem>> {
    let root_user_id = params.root_user_id.unwrap_or_else(|| ctx.uid());
    if root_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let projects = domain()
        .project_manage()
        .list(ctx, &root_user_id, params.status, params.limit)
        .await?;
    let response_items = projects.iter().map(response::to_list_item).collect();

    Ok(response_items)
}