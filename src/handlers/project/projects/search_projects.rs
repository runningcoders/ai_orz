//! Handler: POST /api/v1/projects/search - Search projects with full filtering
//!
//! 与 query_projects 的区别：search 重在"语义相关性"（FTS5 + 向量语义混合搜索），
//! query 重在"条件过滤"。两者现在都支持完整过滤条件和分页返回。

use super::response;
use crate::pkg::RequestContext;
use crate::service::dao::project::{ProjectQuery, ProjectSearch};
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, ProjectListItem, SearchProjectsRequest};
use common::error::Result;

/// Search projects with full filtering (FTS5 + vector semantic search)
#[register_handler_tool(
    id = "search_projects",
    name = "Search Projects",
    description = "Search projects by free-text keyword using hybrid FTS5 full-text and vector semantic matching ranked by relevance, optionally narrowed by ids, root_user_id, status_in, or owner_agent_id. Returns a paged list of project summaries. Prefer this over query_projects when you have natural-language text rather than exact field values.",
    params = "common::api::SearchProjectsRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn search_projects(
    ctx: RequestContext,
    params: SearchProjectsRequest,
) -> Result<PagedResult<ProjectListItem>> {
    let search = ProjectSearch {
        keyword: params.keyword,
        filters: ProjectQuery {
            ids: params.ids,
            root_user_id: params.root_user_id,
            status_in: params.status_in,
            owner_agent_id: params.owner_agent_id,
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().project_manage().search(ctx, search).await?;

    Ok(page.map(|p| response::to_list_item(&p)))
}
