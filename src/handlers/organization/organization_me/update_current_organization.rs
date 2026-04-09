//! 更新当前登录用户所在组织信息接口

use axum::{extract::{Extension, Json}, http::StatusCode};
use crate::{
    error::AppError,
    handlers::ApiResponse,
    pkg::constants::request_context::RequestContext,
    pkg::constants::utils,
    service::domain::organization,
};
use serde::{Deserialize, Serialize};

/// 更新当前组织信息请求
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCurrentOrganizationRequest {
    /// 新的组织名称（None 表示不修改）
    pub name: Option<String>,
    /// 新的组织描述（None 表示不修改）
    pub description: Option<String>,
    /// 新的外部访问地址（None 表示不修改）
    pub base_url: Option<String>,
}

/// 空响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyResponse {
    /// 响应码
    pub code: i32,
    /// 响应消息
    pub message: String,
}

/// Update current authenticated user's organization information
/// 允许管理员更新当前用户所在组织的可修改信息
pub async fn update_current_organization(
    Extension(mut ctx): Extension<RequestContext>,
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
        Json(ApiResponse::success(EmptyResponse {
            code: 0,
            message: "更新成功".to_string(),
        })),
    ))
}
