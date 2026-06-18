//! Handler: PUT /api/v1/projects/{id} - Update project basic information

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{UpdateProjectRequest, UpdateProjectResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use super::response;

/// Update project basic information
#[register_handler_tool(
    id = "update_project",
    name = "update_project",
    description = "Update project basic information (name, description, tags, priority)",
    params = "common::api::UpdateProjectRequest",
)]
#[generate_http_handler]
pub async fn update_project(
    ctx: RequestContext,
    params: UpdateProjectRequest,
) -> Result<UpdateProjectResponse, AppError> {
    let modified_by = ctx.uid();
    let project = domain()
        .project_manage()
        .update_basic(
            ctx,
            &params.id,
            params.name,
            params.description,
            params.priority,
            params.tags,
            modified_by,
        )
        .await?;

    Ok(response::to_detail(&project))
}