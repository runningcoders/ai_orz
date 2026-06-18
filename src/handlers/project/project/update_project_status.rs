//! Handler: PUT /api/v1/projects/{id}/status - Update project status

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{UpdateProjectStatusRequest, UpdateProjectStatusResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use super::response;

/// Update project status
#[register_handler_tool(
    id = "update_project_status",
    name = "update_project_status",
    description = "Update project status (transition to next state)",
    params = "common::api::UpdateProjectStatusRequest",
)]
#[generate_http_handler]
pub async fn update_project_status(
    ctx: RequestContext,
    params: UpdateProjectStatusRequest,
) -> Result<UpdateProjectStatusResponse, AppError> {
    let mut project = domain()
        .project_manage()
        .get(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Project {} not found", params.id)))?;

    domain()
        .project_manage()
        .transition_status(ctx, &mut project, params.status)
        .await?;

    Ok(response::to_detail(&project))
}