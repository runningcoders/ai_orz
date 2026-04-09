//! 获取当前登录用户所在组织信息接口

use axum::{extract::{Extension, Json}, http::StatusCode};
use crate::{
    error::AppError,
    handlers::ApiResponse,
    models::organization::OrganizationPo,
    pkg::constants::request_context::RequestContext,
    service::domain::organization,
};
use serde::{Deserialize, Serialize};

/// 当前组织信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInfoResponse {
    /// 组织 ID
    pub id: String,
    /// 组织名称
    pub name: String,
    /// 组织描述
    pub description: String,
    /// 外部访问地址
    pub base_url: String,
    /// 组织状态
    pub status: i32,
    /// 创建时间戳
    pub created_at: i64,
}

/// 获取当前组织信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentOrganizationResponse {
    /// 组织信息数据
    pub data: OrganizationInfoResponse,
}

/// Get current authenticated user's organization information
/// 从 RequestContext 获取 organization_id，查询组织信息返回
pub async fn get_current_organization(
    Extension(ctx): Extension<RequestContext>,
) -> Result<(StatusCode, Json<ApiResponse<GetCurrentOrganizationResponse>>), AppError> {
    // 从 RequestContext 获取当前组织 ID
    let org_id = ctx.organization_id.clone().ok_or_else(|| {
        AppError::BadRequest("未找到组织信息".to_string())
    })?;

    let domain = organization::domain();
    // 获取组织完整信息
    let org = domain.organization_manage().get_by_id(ctx, &org_id)?
        .ok_or_else(|| AppError::NotFound("组织不存在".to_string()))?;

    // 转换为响应格式
    let info = OrganizationInfoResponse {
        id: org.id.clone(),
        name: org.name.clone(),
        description: org.description.clone(),
        base_url: org.base_url.clone(),
        status: org.status,
        created_at: org.created_at,
    };

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(GetCurrentOrganizationResponse {
            data: info,
        })),
    ))
}
