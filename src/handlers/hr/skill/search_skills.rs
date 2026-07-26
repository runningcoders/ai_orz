//! Handler: GET /api/v1/skills/search - Search skills by keyword with filtering

use crate::pkg::RequestContext;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SearchSkillsRequest, SearchSkillsResponse};
use common::enums::SkillStatus;
use common::error::Result;

use super::response::to_list_item;

/// Search public skills by keyword with optional filtering. Returns matching skills sorted by relevance.
#[register_handler_tool(
    id = "search_skills",
    name = "search_skills",
    description = "Search public skills by keyword with optional filtering. Returns matching skills sorted by relevance.",
    params = "common::api::SearchSkillsRequest",
    neural,
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn search_skills(
    ctx: RequestContext,
    params: SearchSkillsRequest,
) -> Result<SearchSkillsResponse> {
    let skills = domain()
        .skill_manage()
        .search_skills(
            ctx,
            SkillSearch {
                keyword: params.keyword,
                query_vector: None,
                top_k: params.limit.map(|limit| limit as i32),
                vector_distance_threshold: None,
                filters: SkillQuery {
                    status: params.status,
                    exclude_status: params.status.is_none().then_some(SkillStatus::Expired),
                    category: params.category,
                    author_id: params.author_id,
                    pagination: common::api::PaginationParams {
                        limit: params.limit,
                        offset: None,
                    },
                    ..Default::default()
                },
            },
        )
        .await?;

    let skills = skills.iter().map(to_list_item).collect();
    Ok(SearchSkillsResponse { skills })
}
