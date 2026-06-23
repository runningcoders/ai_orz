//! Handler: POST /api/v1/organizations/users - Create a new user in current organization

use crate::error::AppError;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateUserRequest, CreateUserResponse};
use common::enums::UserRole;
use rand::Rng;

/// Create a new user within the current authenticated user's organization
#[register_handler_tool(
    id = "create_user",
    name = "create_user",
    description = "Create a new user in the current organization. Requires admin permissions.",
    params = "common::api::CreateUserRequest"
)]
#[generate_http_handler]
pub async fn create_user(
    ctx: RequestContext,
    params: CreateUserRequest,
) -> Result<CreateUserResponse, AppError> {
    // 从 RequestContext 获取当前组织 ID
    let organization_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("未找到组织信息".to_string()))?;

    let domain = organization::domain();

    // 生成随机用户 ID
    let user_id = generate_id();

    // 转换角色
    let role = match params.role {
        2 => UserRole::Admin,
        1 => UserRole::SuperAdmin,
        _ => UserRole::Member,
    };

    // 创建 UserPo
    let user = UserPo::new(
        user_id.clone(),
        organization_id.clone(),
        params.username.clone(),
        params.display_name.clone().unwrap_or_default(),
        params.email.clone().unwrap_or_default(),
        params.password_hash.clone(),
        role,
        ctx.user_id.clone().unwrap_or_default(),
    );

    domain.user_manage().create_user(ctx, user).await?;

    Ok(CreateUserResponse {
        user_id,
        username: params.username,
        display_name: params.display_name,
        email: params.email,
        role: params.role,
    })
}

/// 生成随机 ID
fn generate_id() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const ID_LEN: usize = 16;
    let mut rng = rand::thread_rng();
    (0..ID_LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
