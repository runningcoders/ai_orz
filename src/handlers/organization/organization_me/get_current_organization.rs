//! Handler: GET /api/v1/organization/me - Get current authenticated user's organization information

use crate::middleware::jwt_auth::expired_jwt_cookie_header_value;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    GetCurrentOrganizationRequest, GetCurrentOrganizationResponse, OrganizationInfoResponse,
};
use common::error::Result;

/// Get information for the currently authenticated user's organization
#[register_handler_tool(
    id = "get_current_organization",
    name = "get_current_organization",
    description = "Get detailed information about the organization that the currently authenticated user belongs to",
    params = "common::api::GetCurrentOrganizationRequest"
)]
#[generate_http_handler]
pub async fn get_current_organization(
    ctx: RequestContext,
    _params: GetCurrentOrganizationRequest,
) -> Result<GetCurrentOrganizationResponse> {
    // 从 RequestContext 获取当前组织 ID
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| common::error::Error::bad_request("未找到组织信息".to_string()))?;

    let domain = organization::domain();
    // 获取组织完整信息
    let org = domain
        .organization_manage()
        .get_by_id(ctx.clone(), &org_id)
        .await?
        // JWT 通过但其引用的 organization_id 在 DB 中已不存在（后端清空数据、
        // 组织被删除等），此时不是 404，而是「会话身份已失效」：返回 401
        // 并附 Set-Cookie 清掉 HttpOnly JWT，前端下一次请求立即出清登录态。
        .ok_or_else(|| {
            common::error::Error::unauthorized(format!(
                "当前登录身份已失效，请重新登录（组织 {org_id} 不存在）"
            ))
            .with_response_header(
                axum::http::header::SET_COOKIE.as_str(),
                expired_jwt_cookie_header_value(),
            )
        })?;

    // 读取组织级配置，随响应一并返回
    let config = domain
        .organization_manage()
        .get_org_config(ctx, &org.id)
        .await?;

    // 转换为响应格式
    let data = OrganizationInfoResponse {
        organization_id: org.id.clone(),
        name: org.name.clone(),
        description: if org.description.is_empty() {
            None
        } else {
            Some(org.description.clone())
        },
        base_url: if org.base_url.is_empty() {
            None
        } else {
            Some(org.base_url.clone())
        },
        status: org.status.to_i32(),
        created_at: org.created_at,
        config,
    };

    Ok(GetCurrentOrganizationResponse { data })
}
