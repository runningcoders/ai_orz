//! 初始化系统接口
//!
//! 当系统还没有初始化时，调用这个接口创建第一个组织和超级管理员

use ai_orz_macros::generate_http_handler;
use common::api::{CheckInitializedRequest, InitializeSystemRequest, InitializeSystemResponse};
use common::error::Result;
use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// 检查系统是否已经初始化
#[generate_http_handler]
pub async fn check_initialized(
    ctx: RequestContext,
    _params: CheckInitializedRequest,
) -> Result<bool> {
    let domain = organization::domain();
    let initialized = domain.organization_manage().check_initialized(ctx).await?;
    Ok(initialized)
}

/// 初始化系统
#[generate_http_handler]
pub async fn initialize_system(
    ctx: RequestContext,
    params: InitializeSystemRequest,
) -> Result<InitializeSystemResponse> {
    let domain = organization::domain();
    let (org_id, user_id) = domain
        .organization_manage()
        .initialize_system(
            ctx,
            params.organization_name,
            params.description,
            params.admin_username,
            params.admin_password_hash,
            params.admin_display_name,
            params.admin_email,
        )
        .await?;

    Ok(InitializeSystemResponse {
        organization_id: org_id,
        user_id: user_id,
    })
}
