//! 调用 Model Provider 生成文本

use common::api::{CallModelRequest, CallModelResponse};
use common::constants::RequestContext;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::service::domain::finance::domain;
use axum::{
    extract::{Extension, Path},
    Json,
};

/// 调用模型
/// POST /model-providers/{id}/call
pub async fn call_model(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<CallModelRequest>,
) -> Result<Json<ApiResponse<CallModelResponse>>, AppError> {

    // 1. 先查询 Model Provider
    let provider = domain().model_provider_manage().get_model_provider(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", id)))?;

    // 2. 调用模型生成结果
    let result = domain().model_provider_manage().wake_cortex(ctx, &provider, &req.prompt)?;

    Ok(Json(ApiResponse::success(CallModelResponse {
        result,
    })))
}
