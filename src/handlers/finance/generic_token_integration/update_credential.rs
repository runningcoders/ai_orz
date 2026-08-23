//! Handler: PATCH /api/v1/finance/identity/generic-token/credentials/{id} - 更新通用 API Token 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::{UpdateCredentialCmd, domain};
use ai_orz_macros::generate_http_handler;
use common::api::{
    UpdateGenericTokenCredentialRequest, UpdateGenericTokenCredentialResponse,
};
use common::error::{Result, bail_err};
use common::models::CredentialDetailPatch;

#[generate_http_handler]
pub async fn update_credential(
    ctx: RequestContext,
    params: UpdateGenericTokenCredentialRequest,
) -> Result<UpdateGenericTokenCredentialResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    domain()
        .identity_credential_manage()
        .update_credential(
            ctx,
            &user_id,
            UpdateCredentialCmd {
                credential_id: params.id,
                name: params.name,
                patch: CredentialDetailPatch::GenericToken {
                    token: params.api_token,
                },
            },
        )
        .await?;

    Ok(UpdateGenericTokenCredentialResponse { success: true })
}
