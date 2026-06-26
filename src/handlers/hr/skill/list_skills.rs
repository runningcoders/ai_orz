//! Handler: GET /api/v1/skills - List skills with optional filtering

use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::dao::skill::SkillQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListSkillsRequest, ListSkillsResponse, SkillListItem};
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
) -> Result<ListSkillsResponse> {
    let skills = domain()
        .skill_manage()
        .query_skills(
            ctx,
            SkillQuery {
                status: params.status,
                exclude_status: params.status.is_none().then_some(SkillStatus::Expired),
                category: params.category,
                author_id: params.author_id,
                keyword: params.keyword,
                limit: params.limit,
                ..Default::default()
            },
        )
        .await?;

    let skills = skills.iter().map(to_list_item).collect();
    Ok(ListSkillsResponse { skills })
}
