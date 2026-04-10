//! 测试 Model Provider 连通性

use common::api::{TestConnectionRequest, TestConnectionResponse};
use common::constants::RequestContext;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::service::domain::finance::domain;
use axum::{
    extract::{Extension, Path},
    Json,
};

/// 测试 Model Provider 连通性
/// POST /model-providers/{id}/test
pub async fn test_model_provider_connection(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<TestConnectionRequest>,
) -> Result<Json<ApiResponse<TestConnectionResponse>>, AppError> {

    // 1. 先查询 Model Provider
    let provider = domain().model_provider_manage().get_model_provider(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", id)))?;

    // 2. 使用 prompt 测试连通性，默认用 "Hello!"
    let prompt = req.prompt.clone().unwrap_or_else(|| "Hello!".to_string());

    match domain().model_provider_manage().wake_cortex(ctx, &provider, &prompt).await {
        Ok(result) => {
            // 如果结果为空也算测试失败
            if result.trim().is_empty() {
                Ok(Json(ApiResponse::success(TestConnectionResponse {
                    success: false,
                    response: Some("模型返回空响应，连通性测试不通过".to_string()),
                    error: None,
                })))
            } else {
                Ok(Json(ApiResponse::success(TestConnectionResponse {
                    success: true,
                    response: Some(result),
                    error: None,
                })))
            }
        }
        Err(e) => {
            Ok(Json(ApiResponse::success(TestConnectionResponse {
                success: false,
                response: None,
                error: Some(e.to_string()),
            })))
        }
    }
}
