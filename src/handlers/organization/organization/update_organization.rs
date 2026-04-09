//! 更新组织信息接口

use common::api::UpdateCurrentOrganizationRequest;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use common::constants::RequestContext;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
    response::IntoResponse,
};
use crate::service::domain::organization;
use std::time::{SystemTime, UNIX_EPOCH};

/// 获取当前时间戳
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// 更新组织信息
pub async fn update_organization(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<UpdateCurrentOrganizationRequest>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    
    // 从 context 获取组织 ID
    let org_id = ctx.organization_id.clone()
        .ok_or_else(|| AppError::BadRequest("未找到组织信息".to_string()))?;
    
    let mut org = domain.organization_manage().get_by_id(ctx.clone(), &org_id)?
        .ok_or_else(|| AppError::NotFound("组织不存在".to_string()))?;
    
    // 更新字段
    if let Some(name) = req.name {
        org.name = name;
    }
    if let Some(description) = req.description {
        org.description = description;
    }
    if let Some(base_url) = req.base_url {
        org.base_url = base_url;
    }
    org.updated_at = current_timestamp();
    
    domain.organization_manage().update(ctx, &org)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(())).into_response()))
}
