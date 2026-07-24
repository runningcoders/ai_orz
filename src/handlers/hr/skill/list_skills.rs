//! Handler: GET /api/v1/skills - List skills with optional filtering

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::skill::SkillQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListSkillsRequest, PagedResult, SkillListItem};
use common::enums::SkillStatus;

use super::response::to_list_item;

/// List public skills with optional filtering by status, category, author, and keyword.
#[register_handler_tool(
    id = "list_skills",
    name = "list_skills",
    description = "List public skills with optional filtering by status, category, author, and keyword.",
    params = "common::api::ListSkillsRequest"
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
