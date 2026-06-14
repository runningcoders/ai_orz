//! 获取单个 Tool

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, GetToolResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 获取 Tool
/// GET /tools/{id}
pub async fn get_tool(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetToolResponse>>, AppError> {
    let tool = domain()
        .tool_provider_manage()
        .get_tool(ctx, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", id)))?;

    Ok(Json(ApiResponse::success(to_detail(&tool))))
}
