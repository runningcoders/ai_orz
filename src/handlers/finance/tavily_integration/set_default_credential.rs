//! Handler: POST /api/v1/finance/identity/tavily/credentials/default - 设置默认 Tavily 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{SetDefaultTavilyCredentialRequest, SetDefaultTavilyCredentialResponse};
use common::error::Result;
use common::models::CredentialKind;

#[generate_http_handler]
pub async fn set_default_credential(
    ctx: RequestContext,
    params: SetDefaultTavilyCredentialRequest,
) -> Result<SetDefaultTavilyCredentialResponse> {
    let user_id = ctx.uid();

    let trimmed = params.credential_id.trim().to_string();
    let target = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };
    domain()
        .identity_credential_manage()
        .set_default_credential(ctx, &user_id, CredentialKind::TavilyKey, target.as_deref())
        .await?;

    Ok(SetDefaultTavilyCredentialResponse { success: true })
}
