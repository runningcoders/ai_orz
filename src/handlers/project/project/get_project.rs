//! Handler: GET /api/v1/projects/{id} - Get project detailed information

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{GetProjectRequest, GetProjectResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use super::response;

/// Get project detailed information
#[register_handler_tool(
    id = "get_project",
    name = "get_project",
    description = "Get project detailed information by ID",
    params = "common::api::GetProjectRequest",
)]
#[generate_http_handler]
pub async fn get_project(
    ctx: RequestContext,
    params: GetProjectRequest,
) -> Result<GetProjectResponse, AppError> {
    let project = domain()
        .project_manage()
        .get(ctx, &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Project {} not found", params.id)))?;

    Ok(response::to_detail(&project))
}