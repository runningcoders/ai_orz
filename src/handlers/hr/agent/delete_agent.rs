//! 删除 Agent

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use crate::service::domain::hr::domain;
use axum::{
    extract::{Path},
    http::HeaderMap,
    Json,
};

/// 删除 Agent
/// DELETE /agents/:id
pub async fn delete_agent(
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let ctx = extract_ctx(&headers);

    let agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    domain().agent_manage().delete_agent(ctx, &agent)?;

    Ok(Json(ApiResponse::<()>::ok()))
}
