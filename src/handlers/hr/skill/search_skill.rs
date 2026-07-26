//! Handler: 搜索技能 - Neural Tool
//!
//! 按关键词或标签搜索技能库，返回技能摘要列表（不含完整内容）。

use crate::pkg::RequestContext;
use crate::service::dao::skill::SkillQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SearchSkillParams, SearchSkillResponse, SkillSummary};
use common::error::Result;

/// Search skills by keyword or tags. Returns skill summaries (id, name, description, tags) without full content.
#[register_handler_tool(
    id = "search_skill",
    name = "search_skill",
    description = "Search skills by keyword or tags. Returns skill summaries (id, name, description, tags) without full content.",
    params = "common::api::SearchSkillParams",
    neural,
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn search_skill(
    ctx: RequestContext,
    params: SearchSkillParams,
) -> Result<SearchSkillResponse> {
    let limit = params.limit.unwrap_or(10);

    let page = domain()
        .skill_manage()
        .query_skills(
            ctx,
            SkillQuery {
                keyword: params.keyword,
                tags: params.tags,
                pagination: common::api::PaginationParams {
                    limit: Some(limit),
                    offset: None,
                },
                ..Default::default()
            },
        )
        .await?;

    let skills = page
        .items
        .iter()
        .map(|s| SkillSummary {
            skill_id: s.po.id.clone(),
            name: s.po.name.clone(),
            description: s.po.description.clone(),
            tags: s.po.parse_tags(),
        })
        .collect();

    Ok(SearchSkillResponse { skills })
}
