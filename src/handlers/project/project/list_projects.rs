//! Handler: GET /api/v1/projects - List all projects for a user with optional filtering

use super::response;
use crate::pkg::RequestContext;
use crate::service::dao::project::ProjectQuery;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListProjectsRequest, ListProjectsResponse, ProjectListItem};
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
) -> Result<ListProjectsResponse> {
    let root_user_id = params.root_user_id.unwrap_or_else(|| ctx.uid());
    if root_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    // 走通用 query（list 是语法糖，handler 内部统一用 query）
    let projects = domain()
        .project_manage()
        .query(
            ctx,
            ProjectQuery {
                root_user_id: Some(root_user_id),
                status_in: params.status.map(|s| vec![s]),
                ids: params.ids,
                limit: params.limit,
                ..Default::default()
            },
        )
        .await?;
    let projects: Vec<ProjectListItem> = projects.iter().map(response::to_list_item).collect();

    Ok(ListProjectsResponse { projects })
}
