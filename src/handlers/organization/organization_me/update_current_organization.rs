//! 更新当前登录用户所在组织信息接口

use axum::{extract::{Extension, Json}, http::StatusCode};
use common::api::{EmptyResponse, UpdateCurrentOrganizationRequest};
use common::constants::{RequestContext, utils};
use crate::{
    error::AppError,
    handlers::ApiResponse,
    service::domain::organization,
};

/// Update current authenticated user's organization information
/// 允许管理员更新当前用户所在组织的可修改信息
pub async fn update_current_organization(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<UpdateCurrentOrganizationRequest>,
) -> Result<(StatusCode, Json<ApiResponse<EmptyResponse>>), AppError> {
    // 从 RequestContext 获取当前组织 ID
    let org_id = ctx.organization_id.clone().ok_or_else(|| {
        AppError::BadRequest("未找到组织信息".to_string())
    })?;

    let domain = organization::domain();
    // 获取当前组织信息
    let mut org = domain.organization_manage().get_by_id(ctx.clone(), &org_id)?
        .ok_or_else(|| AppError::NotFound("组织不存在".to_string()))?;

    // 更新可修改字段
    if let Some(new_name) = req.name {
        org.name = new_name;
    }
    if let Some(new_description) = req.description {
        org.description = new_description;
    }
    if let Some(new_base_url) = req.base_url {
        org.base_url = new_base_url;
    }

    // 更新修改时间
    org.updated_at = utils::current_timestamp();
    if let Some(modifier_id) = ctx.user_id.clone() {
        org.modified_by = modifier_id;
    }

    // 保存更新
    domain.organization_manage().update(ctx, &org)?;

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(EmptyResponse {})),
    ))
}
