//! 列出所有 Model Provider

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::{RequestContext, constants::ProviderType};
use crate::service::domain::finance::domain;
use axum::{
    extract::Extension,
    Json,
};
use serde::{Serialize};

/// Model Provider 列表项响应
#[derive(Debug, Serialize)]
pub struct ModelProviderListItem {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    pub model_name: String,
    pub description: Option<String>,
    pub created_at: i64,
}

/// 列出所有 Model Provider
/// GET /model-providers
pub async fn list_model_providers(
    Extension(ctx): Extension<RequestContext>
) -> Result<Json<ApiResponse<Vec<ModelProviderListItem>>>, AppError> {

    let providers = domain().model_provider_manage().list_model_providers(ctx)?;
    let responses: Vec<ModelProviderListItem> = providers
        .iter()
        .map(|provider| ModelProviderListItem {
            id: provider.po.id.clone(),
            name: provider.po.name.clone(),
            provider_type: provider.po.provider_type.clone(),
            model_name: provider.po.model_name.clone(),
            description: if provider.po.description.is_empty() { None } else { Some(provider.po.description.clone()) },
            created_at: provider.po.created_at,
        })
        .collect();

    Ok(Json(ApiResponse::success(responses)))
}
