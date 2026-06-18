//! Handler: GET /api/v1/projects - List all projects for a user with optional filtering

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{ListProjectsRequest, ProjectListItem};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use super::response;

/// List all projects for a user with optional filtering
#[register_handler_tool(
    id = "list_projects",
    name = "list_projects",
    description = "List all projects for a user with optional status filtering",
    params = "common::api::ListProjectsRequest",
)]
#[generate_http_handler]
pub async fn list_projects(
    ctx: RequestContext,
    params: ListProjectsRequest,
) -> Result<Vec<ProjectListItem>, AppError> {
    let root_user_id = params.root_user_id.unwrap_or_else(|| ctx.uid());
    if root_user_id.is_empty() {
        return Err(AppError::BadRequest("root_user_id 不能为空".to_string()));
    }

    let projects = domain()
        .project_manage()
        .list(ctx, &root_user_id, params.status, params.limit)
        .await?;
    let response_items = projects.iter().map(response::to_list_item).collect();

    Ok(response_items)
}