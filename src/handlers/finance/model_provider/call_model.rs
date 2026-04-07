//! 调用 Model Provider 生成文本

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use crate::service::domain::finance::domain;
use axum::{
    extract::Path,
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

/// 调用模型请求
#[derive(Debug, Deserialize)]
pub struct CallModelRequest {
    /// 调用提示词
    pub prompt: String,
}

/// 调用模型响应
#[derive(Debug, Serialize)]
pub struct CallModelResponse {
    pub result: String,
}

/// 调用模型
/// POST /model-providers/{id}/call
pub async fn call_model(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<CallModelRequest>,
) -> Result<Json<ApiResponse<CallModelResponse>>, AppError> {
    let ctx = extract_ctx(&headers);

    // 1. 先查询 Model Provider
    let provider = domain().model_provider_manage().get_model_provider(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", id)))?;

    // 2. 调用模型生成结果
    let result = domain().model_provider_manage().wake_cortex(ctx, &provider, &req.prompt)?;

    Ok(Json(ApiResponse::success(CallModelResponse {
        result,
    })))
}
