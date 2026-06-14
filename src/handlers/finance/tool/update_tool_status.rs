//! 更新 Tool 状态

use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, UpdateToolStatusRequest, UpdateToolStatusResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 更新 Tool 状态
/// PUT /tools/{id}/status
pub async fn update_tool_status(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateToolStatusRequest>,
) -> Result<AxumJson<ApiResponse<UpdateToolStatusResponse>>, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let mut tool = domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", id)))?;

    tool.transition_status(req.status, user_id)
        .map_err(AppError::BadRequest)?;

    domain()
        .tool_provider_manage()
        .update_tool(ctx, &tool)
        .await?;

    Ok(AxumJson(ApiResponse::success(to_detail(&tool))))
}
