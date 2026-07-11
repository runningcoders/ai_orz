//! Handler: PUT /api/v1/projects/{id}/status - Update project status

use super::response;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateProjectStatusRequest, UpdateProjectStatusResponse};

use crate::enrich_ctx;

/// Update project status
#[register_handler_tool(
    id = "update_project_status",
    name = "update_project_status",
    description = "Update project status (transition to next state)",
    params = "common::api::UpdateProjectStatusRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_project_status(
    ctx: RequestContext,
    params: UpdateProjectStatusRequest,
) -> Result<UpdateProjectStatusResponse> {
    let project = domain()
        .project_manage()
        .get(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Project {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &project);
    let mut project = project;

    domain()
        .project_manage()
        .transition_status(ctx, &mut project, params.status)
        .await?;

    Ok(response::to_detail(&project))
}
