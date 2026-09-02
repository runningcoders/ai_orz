//! Handler: GET /api/v1/skills - List skills with optional filtering

use crate::pkg::RequestContext;
use crate::service::dao::skill::SkillQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListSkillsRequest, PagedResult, SkillListItem};
use common::enums::SkillStatus;
use common::error::Result;

use super::response::to_list_item;

/// List public skills with optional filtering by status, category, author, and keyword.
#[register_handler_tool(
    id = "list_skills",
    name = "List All Skills",
    description = "Browse all skills with plain pagination — no filters available, Expired skills always excluded. Use query_skills for exact field filtering or search_skills for semantic search.",
    params = "common::api::ListSkillsRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn list_skills(
    ctx: RequestContext,
    params: ListSkillsRequest,
) -> Result<PagedResult<SkillListItem>> {
    // list 是语法糖：只接受分页，内部固定排除 Expired
    let page = domain()
        .skill_manage()
        .query_skills(
            ctx,
            SkillQuery {
                exclude_status: Some(SkillStatus::Expired),
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(|s| to_list_item(&s)))
}
