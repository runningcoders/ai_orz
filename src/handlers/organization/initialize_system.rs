//! 初始化系统接口
//!
//! 当系统还没有初始化时，调用这个接口创建第一个组织和超级管理员

use common::api::{InitializeSystemRequest, InitializeSystemResponse};
use crate::pkg::RequestContext;
use crate::error::AppError;
use crate::handlers::ApiResponse;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
    response::IntoResponse,
};
use crate::service::domain::organization;

/// 检查系统是否已经初始化
pub async fn check_initialized(
    Extension(ctx): Extension<RequestContext>,
) -> Result<(StatusCode, Json<ApiResponse<bool>>), AppError> {
    let domain = organization::domain();
    let initialized = domain.organization_manage().check_initialized(ctx).await?;

    Ok((StatusCode::OK, Json(ApiResponse::success(initialized))))
}

/// 初始化系统
pub async fn initialize_system(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<InitializeSystemRequest>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    let (org_id, user_id) = domain.organization_manage().initialize_system(
        ctx,
        req.organization_name.clone(),
        req.description.clone(),
        req.admin_username.clone(),
        req.admin_password_hash.clone(),
        req.admin_display_name.clone(),
        req.admin_email.clone(),
    )
    .await?;

    Ok((StatusCode::OK, Json(ApiResponse::success(InitializeSystemResponse {
        organization_id: org_id,
        user_id: user_id,
    }))).into_response())
}
