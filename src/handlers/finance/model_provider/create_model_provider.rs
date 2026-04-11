//! 创建 Model Provider

use common::api::{CreateModelProviderRequest, CreateModelProviderResponse};
use crate::pkg::RequestContext;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use crate::service::domain::finance::domain;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
};

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
        req.description.clone(),
        ctx.uid().to_string(),
    );
    let provider = ModelProvider::from_po(provider_po);

    domain().model_provider_manage().create_model_provider(ctx, &provider).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(CreateModelProviderResponse {
            id: provider.po.id.clone().expect("id should not be None"),
            name: provider.po.name.clone().expect("name should not be None"),
            provider_type: provider.po.provider_type.clone(),
            model_name: provider.po.model_name.clone().expect("model_name should not be None"),
            description: if provider.po.description.as_ref().map_or(true, |d| d.is_empty()) { None } else { provider.po.description.clone() },
            created_at: provider.po.created_at,
        })),
    ))
}
