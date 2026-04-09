//! 获取单个 Model Provider

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::{RequestContext, constants::ProviderType};
use crate::service::domain::finance::domain;
use axum::{
    extract::{Extension, Path},
    Json,
};
use serde::{Serialize};

/// 获取 Model Provider 响应
#[derive(Debug, Serialize)]
pub struct GetModelProviderResponse {
    pub id: String,
    pub name: String,
    provider_type: ProviderType,
    pub model_name: String,
    pub base_url: Option<String>,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 获取 Model Provider
/// GET /model-providers/{id}
pub async fn get_model_provider(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<GetModelProviderResponse>>, AppError> {

    let provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx, &id)?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", id)))?;

    Ok(Json(ApiResponse::success(GetModelProviderResponse {
        id: provider.po.id.clone(),
        name: provider.po.name.clone(),
        provider_type: provider.po.provider_type.clone(),
        model_name: provider.po.model_name.clone(),
        base_url: provider.po.base_url.clone(),
        description: if provider.po.description.is_empty() { None } else { Some(provider.po.description.clone()) },
        created_at: provider.po.created_at,
        updated_at: provider.po.updated_at,
    })))
}
