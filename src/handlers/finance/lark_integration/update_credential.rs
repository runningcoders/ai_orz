//! Handler: PUT /api/v1/finance/identity/lark/credentials/{id} - 更新飞书应用凭证
//!
//! 变更联动由 Domain 编排：清 HOME lark-cli config + WS 重建联。

use crate::pkg::RequestContext;
use crate::service::domain::finance::{UpdateCredentialCmd, domain};
use ai_orz_macros::generate_http_handler;
use common::api::{UpdateLarkCredentialRequest, UpdateLarkCredentialResponse};
use common::error::{Result, bail_err};
use common::models::CredentialDetailPatch;

#[generate_http_handler]
pub async fn update_credential(
    ctx: RequestContext,
    params: UpdateLarkCredentialRequest,
) -> Result<UpdateLarkCredentialResponse> {
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
                patch: CredentialDetailPatch::LarkApp {
                    app_id: params.app_id,
                    app_secret: params.app_secret,
                    encrypt_key: params.encrypt_key,
                    verification_token: params.verification_token,
                },
            },
        )
        .await?;

    Ok(UpdateLarkCredentialResponse { success: true })
}
