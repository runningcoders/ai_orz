//! 更新 Model Provider

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::{RequestContext, constants::ProviderType};
use crate::service::domain::finance::domain;
use axum::{
    extract::{Extension, Path, Json},
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// 更新 Model Provider 请求
#[derive(Debug, Deserialize)]
pub struct UpdateModelProviderRequest {
    /// Provider 名称
    pub name: Option<String>,
    /// Provider 类型
    pub provider_type: Option<ProviderType>,
    /// 模型名称
    pub model_name: Option<String>,
    /// API Key
    pub api_key: Option<String>,
    /// 自定义 Base URL
    pub base_url: Option<String>,
    /// 描述
    pub description: Option<String>,
}

/// 更新 Model Provider 响应
#[derive(Debug, Serialize)]
pub struct UpdateModelProviderResponse {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    pub model_name: String,
    pub base_url: Option<String>,
    pub description: Option<String>,
    pub updated_at: i64,
}

/// 获取当前时间戳
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// 更新 Model Provider
/// PUT /model-providers/{id}
pub async fn update_model_provider(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateModelProviderRequest>,
) -> Result<Json<ApiResponse<UpdateModelProviderResponse>>, AppError> {

    let mut provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &id)?
        .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", id)))?;

    // 更新字段
    if let Some(name) = req.name {
        provider.po.name = name;
    }
    if let Some(provider_type) = req.provider_type {
        provider.po.provider_type = provider_type;
    }
    if let Some(model_name) = req.model_name {
        provider.po.model_name = model_name;
    }
    if let Some(api_key) = req.api_key {
        provider.po.api_key = api_key;
    }
    if let Some(base_url) = req.base_url {
        provider.po.base_url = Some(base_url);
    }
    if let Some(description) = req.description {
        provider.po.description = description;
    }
    // 更新 modified_by 和 updated_at
    provider.po.modified_by = ctx.uid();
    provider.po.updated_at = current_timestamp();

    domain().model_provider_manage().update_model_provider(ctx, &provider)?;

    Ok(Json(ApiResponse::success(UpdateModelProviderResponse {
        id: provider.po.id.clone(),
        name: provider.po.name.clone(),
        provider_type: provider.po.provider_type.clone(),
        model_name: provider.po.model_name.clone(),
        base_url: provider.po.base_url.clone(),
        description: if provider.po.description.is_empty() { None } else { Some(provider.po.description.clone()) },
        updated_at: provider.po.updated_at,
    })))
}
