//! 获取 Skill

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, GetSkillResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

use super::response::to_detail;

/// 获取 Skill
/// GET /hr/skills/{id}
pub async fn get_skill(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetSkillResponse>>, AppError> {
    let skill = domain()
        .skill_manage()
        .get_skill(ctx, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Skill {} not found", id)))?;

    Ok(Json(ApiResponse::success(to_detail(&skill))))
}
