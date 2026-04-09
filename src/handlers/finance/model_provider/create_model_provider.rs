//! 创建 Model Provider

use common::constants::{RequestContext, ProviderType};
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use crate::service::domain::finance::domain;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

/// 创建 Model Provider 请求
#[derive(Debug, Deserialize)]
pub struct CreateModelProviderRequest {
    /// Provider 名称
    pub name: String,
    /// Provider 类型
    pub provider_type: ProviderType,
    /// 模型名称
    pub model_name: String,
    /// API Key
    pub api_key: String,
    /// 自定义 Base URL
    pub base_url: Option<String>,
    /// 描述
    pub description: Option<String>,
}

/// 创建 Model Provider 响应
#[derive(Debug, Serialize)]
pub struct CreateModelProviderResponse {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    pub model_name: String,
    pub description: Option<String>,
    pub created_at: i64,
}

/// 创建 Model Provider
/// POST /model-providers
pub async fn create_model_provider(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<CreateModelProviderRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateModelProviderResponse>>), AppError> {

    let provider_po = ModelProviderPo::new(
        req.name.clone(),
        req.provider_type.clone(),
        req.model_name.clone(),
        req.api_key.clone(),
        req.base_url.clone(),
        req.description.unwrap_or_default(),
        ctx.uid().to_string(),
    );
    let provider = ModelProvider::from_po(provider_po);

    domain().model_provider_manage().create_model_provider(ctx, &provider)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(CreateModelProviderResponse {
            id: provider.po.id.clone(),
            name: provider.po.name.clone(),
            provider_type: provider.po.provider_type.clone(),
            model_name: provider.po.model_name.clone(),
            description: if provider.po.description.is_empty() { None } else { Some(provider.po.description.clone()) },
            created_at: provider.po.created_at,
        })),
    ))
}
