//! Handler: POST /api/v1/projects - Create a new project

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateProjectRequest, CreateProjectResponse};
use common::error::{Result, bail_err};

/// Create a new project
///
/// handler 纯粹透传 `owner_agent_id`：不调 resolve_agent，不依赖 hr domain。
/// owner_agent_id 由上层（前端/A2A handler）按需组合传入。
#[register_handler_tool(
    id = "create_project",
    name = "Create Project",
    description = "Create a new project",
    params = "common::api::CreateProjectRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn create_project(
    ctx: RequestContext,
    params: CreateProjectRequest,
) -> Result<CreateProjectResponse> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let project = domain()
        .project_manage()
        .create(
            ctx,
            params.name,
            params.description.unwrap_or_default(),
            params.priority.unwrap_or_default(),
            params.tags.unwrap_or_default(),
            params.owner_agent_id, // 纯粹透传，不在此处调 resolve_agent
            current_user_id.clone(),
            current_user_id,
        )
        .await?;

    Ok(response::to_detail(&project))
}
