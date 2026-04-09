//! 初始化系统接口
//!
//! 当系统还没有初始化时，调用这个接口创建第一个组织和超级管理员

use crate::error::AppError;
use crate::handlers::ApiResponse;
use crate::pkg::RequestContext;
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::service::domain::organization;

/// 初始化系统请求
#[derive(Debug, Deserialize)]
pub struct InitializeSystemRequest {
    /// 组织名称
    pub organization_name: String,
    /// 组织描述
    pub description: Option<String>,
    /// 超级管理员用户名
    pub username: String,
    /// 超级管理员密码哈希（bcrypt 哈希后传递）
    pub password_hash: String,
    /// 超级管理员显示名称
    pub display_name: Option<String>,
    /// 超级管理员邮箱
    pub email: Option<String>,
}

/// 初始化系统响应
#[derive(Debug, Serialize)]
pub struct InitializeSystemResponse {
    /// 组织 ID
    pub organization_id: String,
    /// 超级管理员用户 ID
    pub user_id: String,
}

/// 检查系统是否已经初始化
pub async fn check_initialized(
    Extension(ctx): Extension<RequestContext>,
) -> Result<(StatusCode, Json<ApiResponse<bool>>), AppError> {
    let domain = organization::domain();
    let initialized = domain.organization_manage().check_initialized(ctx)?;

    Ok((StatusCode::OK, Json(ApiResponse::success(initialized))))
}

/// 初始化系统
pub async fn initialize_system(
    Extension(ctx): Extension<RequestContext>,
    req: Json<InitializeSystemRequest>,
) -> Result<impl IntoResponse, AppError> {
    let domain = organization::domain();
    let (org_id, user_id) = domain.organization_manage().initialize_system(
        ctx,
        req.organization_name.clone(),
        req.description.clone(),
        req.username.clone(),
        req.password_hash.clone(),
        req.display_name.clone(),
        req.email.clone(),
    )?;

    Ok((StatusCode::OK, Json(ApiResponse::success(InitializeSystemResponse {
        organization_id: org_id,
        user_id: user_id,
    }))).into_response())
}
