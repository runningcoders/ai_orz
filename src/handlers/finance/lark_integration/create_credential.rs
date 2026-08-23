//! Handler: POST /api/v1/finance/identity/lark/credentials - 手动录入创建飞书应用凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::{CreateCredentialCmd, domain};
use ai_orz_macros::generate_http_handler;
use common::api::{CreateLarkCredentialRequest, CreateLarkCredentialResponse};
use common::error::{Result, bail_err};
use common::models::CredentialDetail;

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
        .create_credential(
            ctx,
            &user_id,
            CreateCredentialCmd {
                name: params.name,
                detail: CredentialDetail::LarkApp {
                    app_id: params.app_id,
                    app_secret: params.app_secret,
                    encrypt_key: params.encrypt_key,
                    verification_token: params.verification_token,
                },
                platform: None,
            },
        )
        .await?;

    Ok(CreateLarkCredentialResponse { credential_id })
}
