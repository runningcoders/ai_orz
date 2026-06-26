//! Handler: GET /api/v1/projects/{id} - Get project detailed information

use super::response;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetProjectRequest, GetProjectResponse};

/// Get project detailed information
#[register_handler_tool(
    id = "get_project",
    name = "get_project",
    description = "Get project detailed information by ID",
    params = "common::api::GetProjectRequest"
)]
#[generate_http_handler]
pub async fn get_project(
    ctx: RequestContext,
    params: GetProjectRequest,
) -> Result<GetProjectResponse> {
    let project = domain()
        .project_manage()
        .get(ctx, &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Project {} not found", params.id)))?;

    Ok(response::to_detail(&project))
}
