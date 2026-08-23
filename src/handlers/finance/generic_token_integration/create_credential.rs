//! Handler: POST /api/v1/finance/identity/generic-token/credentials - 创建通用 API Token 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::{CreateCredentialCmd, domain};
use ai_orz_macros::generate_http_handler;
use common::api::{CreateGenericTokenCredentialRequest, CreateGenericTokenCredentialResponse};
use common::error::{Result, bail_err};
use common::models::CredentialDetail;

#[generate_http_handler]
pub async fn create_credential(
    ctx: RequestContext,
    params: CreateGenericTokenCredentialRequest,
) -> Result<CreateGenericTokenCredentialResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let platform = params.platform.trim().to_string();
    if platform.is_empty() {
        bail_err!(InvalidRequest, "platform 不能为空");
    }

    let credential_id = domain()
        .identity_credential_manage()
        .create_credential(
            ctx,
            &user_id,
            CreateCredentialCmd {
                name: params.name,
                detail: CredentialDetail::GenericToken {
                    token: params.api_token,
                },
                platform: Some(platform),
            },
        )
        .await?;

    Ok(CreateGenericTokenCredentialResponse { credential_id })
}
