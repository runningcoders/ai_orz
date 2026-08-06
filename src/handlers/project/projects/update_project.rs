//! Handler: PUT /api/v1/projects/{id} - Update project basic information

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateProjectRequest, UpdateProjectResponse};
use common::error::Result;

/// Update project basic information
#[register_handler_tool(
    id = "update_project",
    name = "update_project",
    description = "Update project basic information (name, description, tags, priority)",
    params = "common::api::UpdateProjectRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_project(
    ctx: RequestContext,
    params: UpdateProjectRequest,
) -> Result<UpdateProjectResponse> {
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
            params.execution_plan,
            params.execution_result,
            modified_by,
        )
        .await?;

    Ok(response::to_detail(&project))
}
