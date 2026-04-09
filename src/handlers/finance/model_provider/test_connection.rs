//! 测试 Model Provider 连通性

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use axum::{
    extract::{Extension, Path},
    Json,
};
use serde::{Deserialize, Serialize};

/// 测试 Model Provider 连通性请求
#[derive(Debug, Deserialize)]
pub struct TestModelProviderConnectionRequest {
    // 测试请求目前不需要额外参数，只需要 ID 在路径
}

/// 测试 Model Provider 连通性响应
#[derive(Debug, Serialize)]
pub struct TestModelProviderConnectionResponse {
    pub success: bool,
    pub message: String,
    pub result: Option<String>,
}

/// 测试 Model Provider 连通性
/// POST /model-providers/{id}/test
pub async fn test_model_provider_connection(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TestModelProviderConnectionResponse>>, AppError> {

    // 1. 先查询 Model Provider
    let provider = domain().model_provider_manage().get_model_provider(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", id)))?;

    // 2. 使用默认 prompt "Hello!" 测试连通性
    match domain().model_provider_manage().wake_cortex(ctx, &provider, "Hello!") {
        Ok(result) => {
            // 如果结果为空也算测试失败
            if result.trim().is_empty() {
                let resp = TestModelProviderConnectionResponse {
                    success: false,
                    message: "模型返回空响应，连通性测试不通过".to_string(),
                    result: None,
                };
                Ok(Json(ApiResponse {
                    code: 400,
                    message: "模型返回空响应".to_string(),
                    data: Some(resp),
                }))
            } else {
                Ok(Json(ApiResponse::success(TestModelProviderConnectionResponse {
                    success: true,
                    message: "连通性测试成功".to_string(),
                    result: Some(result),
                })))
            }
        }
        Err(e) => {
            let resp = TestModelProviderConnectionResponse {
                success: false,
                message: e.to_string(),
                result: None,
            };
            Ok(Json(ApiResponse {
                code: e.code(),
                message: e.to_string(),
                data: Some(resp),
            }))
        }
    }
}
