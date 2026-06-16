//! 删除 Skill

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::ApiResponse;

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

/// 删除 Skill
/// DELETE /hr/skills/{id}
pub async fn delete_skill(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    domain()
        .skill_manage()
        .get_skill(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Skill {} not found", id)))?;

    domain().skill_manage().delete_skill(ctx, &id).await?;

    Ok(Json(ApiResponse::<()>::ok()))
}
