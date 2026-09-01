//! Handler: GET /api/v1/projects - List all projects for a user with optional filtering

use super::response;
use crate::pkg::RequestContext;
use crate::service::dao::project::ProjectQuery;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListProjectsRequest, PagedResult, ProjectListItem};
use common::error::{Result, bail_err};

/// List all projects for a user with optional filtering
#[register_handler_tool(
    id = "list_projects",
    name = "List All Projects",
    description = "List all projects for a user with optional status filtering",
    params = "common::api::ListProjectsRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn list_projects(
    ctx: RequestContext,
    params: ListProjectsRequest,
) -> Result<PagedResult<ProjectListItem>> {
    // list 是语法糖：只接受分页，内部固定 root_user_id=ctx.uid() + 排除 status=0
    let root_user_id = ctx.uid();
    if root_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let page = domain()
        .project_manage()
        .query(
            ctx,
            ProjectQuery {
                root_user_id: Some(root_user_id),
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(|p| response::to_list_item(&p)))
}
