//! 搜索 Skill

use axum::{
    Json,
    extract::{Extension, Query},
};
use common::api::{ApiResponse, SkillListItem, SkillSearchQuery};
use common::enums::SkillStatus;

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use crate::service::domain::hr::domain;

use super::response::to_list_item;

/// 搜索 Skill
/// GET /hr/skills/search
pub async fn search_skills(
    Extension(ctx): Extension<RequestContext>,
    Query(req): Query<SkillSearchQuery>,
) -> Result<Json<ApiResponse<Vec<SkillListItem>>>, AppError> {
    let skills = domain()
        .skill_manage()
        .search_skills(
            ctx,
            SkillSearch {
                keyword: req.keyword,
                query_vector: None,
                top_k: req.limit.map(|limit| limit as i32),
                filters: SkillQuery {
                    status: req.status,
                    exclude_status: req.status.is_none().then_some(SkillStatus::Expired),
                    category: req.category,
                    limit: req.limit,
                    ..Default::default()
                },
            },
        )
        .await?;

    let responses = skills.iter().map(to_list_item).collect();
    Ok(Json(ApiResponse::success(responses)))
}
