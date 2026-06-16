//! 列出 Skill

use axum::{
    Json,
    extract::{Extension, Query},
};
use common::api::{ApiResponse, SkillListItem, SkillListQuery};
use common::enums::SkillStatus;

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::dao::skill::SkillQuery;
use crate::service::domain::hr::domain;

use super::response::to_list_item;

/// 列出 Skill
/// GET /hr/skills
pub async fn list_skills(
    Extension(ctx): Extension<RequestContext>,
    Query(req): Query<SkillListQuery>,
) -> Result<Json<ApiResponse<Vec<SkillListItem>>>, AppError> {
    let skills = domain()
        .skill_manage()
        .query_skills(
            ctx,
            SkillQuery {
                status: req.status,
                exclude_status: req.status.is_none().then_some(SkillStatus::Expired),
                category: req.category,
                author_id: req.author_id,
                keyword: req.keyword,
                limit: req.limit,
                ..Default::default()
            },
        )
        .await?;

    let responses = skills.iter().map(to_list_item).collect();
    Ok(Json(ApiResponse::success(responses)))
}
