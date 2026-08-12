//! Handler: PUT /api/v1/finance/identity/lark/credentials/{id} - 更新飞书应用凭证
//!
//! 变更联动由 Domain 编排：清 HOME lark-cli config + WS 重建联。

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{UpdateLarkCredentialRequest, UpdateLarkCredentialResponse};
use common::error::{Result, bail_err};

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
        .update_lark_credential(
            ctx,
            &user_id,
            &params.id,
            params.name.as_deref(),
            params.app_id.as_deref(),
            params.app_secret.as_deref(),
            params.encrypt_key.as_deref(),
            params.verification_token.as_deref(),
        )
        .await?;

    Ok(UpdateLarkCredentialResponse { success: true })
}
