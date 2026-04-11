//! 列出所有 Model Provider

use common::api::ModelProviderListItem;
use crate::pkg::RequestContext;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::service::domain::finance::domain;
use axum::{
    extract::Extension,
    Json,
};

/// 列出所有 Model Provider
/// GET /model-providers
pub async fn list_model_providers(
    Extension(ctx): Extension<RequestContext>
) -> Result<Json<ApiResponse<Vec<ModelProviderListItem>>>, AppError> {

    let providers = domain().model_provider_manage().list_model_providers(ctx).await?;
    let responses: Vec<ModelProviderListItem> = providers
        .iter()
        .map(|provider| ModelProviderListItem {
            id: provider.po.id.clone().expect("id should not be None"),
            name: provider.po.name.clone().expect("name should not be None"),
            provider_type: provider.po.provider_type.clone(),
            model_name: provider.po.model_name.clone().expect("model_name should not be None"),
            description: if provider.po.description.as_ref().map_or(true, |d| d.is_empty()) { None } else { provider.po.description.clone() },
            created_at: provider.po.created_at,
        })
        .collect();

    Ok(Json(ApiResponse::success(responses)))
}
