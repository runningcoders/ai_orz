//! 测试 Model Provider 连通性

use crate::error::AppError;
use crate::handlers::{ApiResponse, extract_ctx};
use crate::service::domain::finance::domain;
use axum::{
    extract::Path,
    http::HeaderMap,
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
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TestModelProviderConnectionResponse>>, AppError> {
    let ctx = extract_ctx(&headers);

    match domain().model_provider_manage().test_model_provider_connection(ctx, &id) {
        Ok(result) => Ok(Json(ApiResponse::success(TestModelProviderConnectionResponse {
            success: true,
            message: "连通性测试成功".to_string(),
            result: Some(result),
        }))),
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
