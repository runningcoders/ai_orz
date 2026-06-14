//! 删除 Tool

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::ApiResponse;

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

/// 删除 Tool
/// DELETE /tools/{id}
pub async fn delete_tool(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let tool = domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", id)))?;

    domain()
        .tool_provider_manage()
        .delete_tool(ctx, &tool)
        .await?;

    Ok(Json(ApiResponse::<()>::ok()))
}
