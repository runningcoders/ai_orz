//! Handler: POST /api/v1/finance/identity/tavily/credentials - 手动录入创建 Tavily 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::{CreateCredentialCmd, domain};
use ai_orz_macros::generate_http_handler;
use common::api::{CreateTavilyCredentialRequest, CreateTavilyCredentialResponse};
use common::error::{Result, bail_err};
use common::models::CredentialDetail;

#[generate_http_handler]
pub async fn create_credential(
    ctx: RequestContext,
    params: CreateTavilyCredentialRequest,
) -> Result<CreateTavilyCredentialResponse> {
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
                detail: CredentialDetail::TavilyKey {
                    api_key: params.api_key,
                },
            },
        )
        .await?;

    Ok(CreateTavilyCredentialResponse { credential_id })
}
