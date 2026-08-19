//! Handler: PUT /api/v1/finance/identity/tavily/credentials/{id} - 更新 Tavily 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::{UpdateCredentialCmd, domain};
use ai_orz_macros::generate_http_handler;
use common::api::{UpdateTavilyCredentialRequest, UpdateTavilyCredentialResponse};
use common::error::Result;
use common::models::CredentialDetailPatch;

#[generate_http_handler]
pub async fn update_credential(
    ctx: RequestContext,
    params: UpdateTavilyCredentialRequest,
) -> Result<UpdateTavilyCredentialResponse> {
    let user_id = ctx.uid();

    domain()
        .identity_credential_manage()
        .update_credential(
            ctx,
            &user_id,
            UpdateCredentialCmd {
                credential_id: params.id,
                name: params.name,
                patch: CredentialDetailPatch::TavilyKey {
                    api_key: params.api_key,
                },
            },
        )
        .await?;

    Ok(UpdateTavilyCredentialResponse { success: true })
}
