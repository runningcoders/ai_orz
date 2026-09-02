//! Handler: POST /api/v1/skills/search - Search skills with full filtering
//!
//! 与 query_skills 的区别：search 重在"语义相关性"（FTS5 + 向量语义混合搜索），
//! query 重在"条件过滤"。两者现在都支持完整过滤条件和分页返回。

use crate::pkg::RequestContext;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, SearchSkillsRequest, SkillListItem};
use common::enums::SkillStatus;
use common::error::Result;

use super::response::to_list_item;

/// Search skills with full filtering (FTS5 + vector semantic search)
#[register_handler_tool(
    id = "search_skills",
    name = "Search Skills (Semantic)",
    description = "Hybrid-search skills by free-text keyword using FTS5 full-text plus vector semantic matching, ranked by relevance, with the same structured filters and pagination as query_skills. Use query_skills when you know exact field values instead.",
    params = "common::api::SearchSkillsRequest",
    neural,
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn search_skills(
    ctx: RequestContext,
    params: SearchSkillsRequest,
) -> Result<PagedResult<SkillListItem>> {
    let search = SkillSearch {
        keyword: params.keyword,
        filters: SkillQuery {
            ids: params.ids,
            status: params.status,
            exclude_status: params.status.is_none().then_some(SkillStatus::Expired),
            category: params.category,
            author_id: params.author_id,
            author_type: params.author_type,
            parent_skill_id: params.parent_skill_id,
            tags: params.tags,
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().skill_manage().search_skills(ctx, search).await?;

    Ok(page.map(|s| to_list_item(&s)))
}
