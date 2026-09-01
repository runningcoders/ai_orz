//! Handler: POST /api/v1/projects/query - Project 通用查询接口
//!
//! 与 list_projects 的区别：list 是列表场景语法糖（GET + query param），
//! query 是完整查询能力（POST + body），支持复杂组合过滤。

use super::response;
use crate::pkg::RequestContext;
use crate::service::dao::project::ProjectQuery;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, ProjectListItem, ProjectQueryRequest};
use common::error::Result;

/// Project 通用查询（POST body，支持完整查询能力）
#[register_handler_tool(
    id = "query_projects",
    name = "Query Projects (Advanced)",
    description = "Query projects with full filtering support (ids, keyword, status, root_user_id)",
    params = "common::api::ProjectQueryRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn query_projects(
    ctx: RequestContext,
    params: ProjectQueryRequest,
) -> Result<PagedResult<ProjectListItem>> {
    let page = domain()
        .project_manage()
        .query(
            ctx,
            ProjectQuery {
                ids: params.ids,
                keyword: params.keyword,
                root_user_id: params.root_user_id,
                status_in: params.status_in,
                owner_agent_id: params.owner_agent_id,
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(|p| response::to_list_item(&p)))
}
