//! Handler: POST /api/v1/finance/identity/github/credentials/default - 设置默认 GitHub 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{SetDefaultGithubCredentialRequest, SetDefaultGithubCredentialResponse};
use common::error::{Result, bail_err};
use common::models::CredentialKind;

#[generate_http_handler]
pub async fn set_default_credential(
    ctx: RequestContext,
    params: SetDefaultGithubCredentialRequest,
) -> Result<SetDefaultGithubCredentialResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let trimmed = params.credential_id.trim().to_string();
    let target = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };
    domain()
        .identity_credential_manage()
        .set_default_credential(
            ctx,
            &user_id,
            CredentialKind::GithubToken,
            target.as_deref(),
        )
        .await?;

    Ok(SetDefaultGithubCredentialResponse { success: true })
}
