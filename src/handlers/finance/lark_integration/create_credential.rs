//! Handler: POST /api/v1/finance/identity/lark/credentials - 手动录入创建飞书应用凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{CreateLarkCredentialRequest, CreateLarkCredentialResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn create_credential(
    ctx: RequestContext,
    params: CreateLarkCredentialRequest,
) -> Result<CreateLarkCredentialResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let credential_id = domain()
        .identity_credential_manage()
        .create_lark_credential(
            ctx,
            &user_id,
            &params.name,
            &params.app_id,
            &params.app_secret,
            params.encrypt_key.as_deref(),
            params.verification_token.as_deref(),
        )
        .await?;

    Ok(CreateLarkCredentialResponse { credential_id })
}
