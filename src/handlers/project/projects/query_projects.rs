//! Handler: POST /api/v1/projects/query - Project 通用查询接口
//!
//! 与 list_projects 的区别：list 是列表场景语法糖（GET + query param），
//! query 是完整查询能力（POST + body），支持复杂组合过滤。

use super::response;
use super::scope::scoped_root_user_id;
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
    description = "Filter projects by structured fields with pagination: ids, status_in (OR list), owner_agent_id, and an optional keyword. Results are always scoped to the calling user's own projects (root_user_id may only be your own; omitting it means your own). Use it when you know exact field values; use list_projects for unfiltered browsing and search_projects for relevance-ranked keyword search.",
    params = "common::api::ProjectQueryRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn query_projects(
    ctx: RequestContext,
    params: ProjectQueryRequest,
) -> Result<PagedResult<ProjectListItem>> {
    // 读侧用户域收敛：只能查自己的项目（缺省即自己，传他人 ID 直接拒绝）
    let root_user_id = scoped_root_user_id(&ctx, params.root_user_id)?;

    let page = domain()
        .project_manage()
        .query(
            ctx,
            ProjectQuery {
                ids: params.ids,
                keyword: params.keyword,
                root_user_id,
                status_in: params.status_in,
                owner_agent_id: params.owner_agent_id,
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(|p| response::to_list_item(&p)))
}
