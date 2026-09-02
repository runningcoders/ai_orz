//! Handler: POST /api/v1/hr/skills/query - Skill 通用查询接口
//!
//! 与 list_skills 的区别：list 是列表场景语法糖（GET + query param），
//! query 是完整查询能力（POST + body），支持复杂组合过滤。

use crate::pkg::RequestContext;
use crate::service::dao::skill::SkillQuery;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, SkillListItem, SkillQueryRequest};
use common::enums::SkillStatus;
use common::error::Result;

use super::response::to_list_item;

/// Skill 通用查询（POST body，支持完整查询能力）
#[register_handler_tool(
    id = "query_skills",
    name = "Query Skills (Advanced)",
    description = "Filter skills by exact structured fields: ids, keyword, status, category, tags, author_id, author_type, or parent_skill_id. Expired skills are excluded unless you explicitly filter for that status. Use list_skills for plain browsing or search_skills for semantic search.",
    params = "common::api::SkillQueryRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn query_skills(
    ctx: RequestContext,
    params: SkillQueryRequest,
) -> Result<PagedResult<SkillListItem>> {
    let page = domain()
        .skill_manage()
        .query_skills(
            ctx,
            SkillQuery {
                ids: params.ids,
                keyword: params.keyword,
                status: params.status,
                exclude_status: params.status.is_none().then_some(SkillStatus::Expired),
                category: params.category,
                author_id: params.author_id,
                author_type: params.author_type,
                parent_skill_id: params.parent_skill_id,
                has_parent: None,
                tags: params.tags,
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(|s| to_list_item(&s)))
}
