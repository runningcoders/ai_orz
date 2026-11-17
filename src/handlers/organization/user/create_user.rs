//! Handler: POST /api/v1/organizations/users - Create a new user in current organization

use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::generate_http_handler;
use common::api::{CreateUserRequest, CreateUserResponse};
use common::enums::UserRole;
use common::error::Result;

/// Create a new user within the current authenticated user's organization
/// 注意：此 handler 不注册为 Agent 工具（涉及密码哈希等敏感字段，仅管理员手动调用）。
#[generate_http_handler]
pub async fn create_user(
    ctx: RequestContext,
    params: CreateUserRequest,
) -> Result<CreateUserResponse> {
    // 从 RequestContext 获取当前组织 ID
    let organization_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| common::error::Error::bad_request("未找到组织信息".to_string()))?;

    let domain = organization::domain();

    // 生成随机用户 ID（格式集中由 common::id 管理）
    let user_id = common::id::UserId::generate().to_string();

    // 修复 E2E-4：之前手写映射（1=SuperAdmin/2=Admin）与 UserRole 枚举值（0/1/2）不一致，
    // 导致前端提交的角色被错误转译。统一用 UserRole::from_i32（非法值落 Member），
    // 响应回传规范化后的枚举值而非原始入参
    let role = UserRole::from_i32(params.role);

    // 创建 UserPo
    let user = UserPo::new(
        user_id.clone(),
        organization_id.clone(),
        params.username.clone(),
        params.display_name.clone().unwrap_or_default(),
        params.email.clone().unwrap_or_default(),
        // 服务端唯一哈希点：入参为明文密码
        crate::pkg::password::hash_password(&params.password)?,
        role,
        ctx.user_id.clone().unwrap_or_default(),
    );

    domain.user_manage().create_user(ctx, user).await?;

    Ok(CreateUserResponse {
        user_id,
        username: params.username,
        display_name: params.display_name,
        email: params.email,
        role: role.to_i32(),
    })
}
