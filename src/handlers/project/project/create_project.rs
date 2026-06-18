//! Handler: POST /api/v1/projects - Create a new project

use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{CreateProjectRequest, CreateProjectResponse};
use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use super::response;

/// Create a new project
#[register_handler_tool(
    id = "create_project",
    name = "create_project",
    description = "Create a new project",
    params = "common::api::CreateProjectRequest",
)]
#[generate_http_handler]
pub async fn create_project(
    ctx: RequestContext,
    params: CreateProjectRequest,
) -> Result<CreateProjectResponse, AppError> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        return Err(AppError::BadRequest("当前用户不能为空".to_string()));
    }

    let project = domain()
        .project_manage()
        .create(
            ctx,
            params.name,
            params.description.unwrap_or_default(),
            params.priority.unwrap_or_default(),
            params.tags.unwrap_or_default(),
            current_user_id.clone(),
            current_user_id,
        )
        .await?;

    Ok(response::to_detail(&project))
}