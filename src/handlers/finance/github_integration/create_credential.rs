//! Handler: POST /api/v1/finance/identity/github/credentials - 手动录入创建 GitHub 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::{CreateCredentialCmd, domain};
use ai_orz_macros::generate_http_handler;
use common::api::{CreateGithubCredentialRequest, CreateGithubCredentialResponse};
use common::error::{Result, bail_err};
use common::models::CredentialDetail;

#[generate_http_handler]
pub async fn create_credential(
    ctx: RequestContext,
    params: CreateGithubCredentialRequest,
) -> Result<CreateGithubCredentialResponse> {
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
                detail: CredentialDetail::GithubToken {
                    token: params.token,
                },
            },
        )
        .await?;

    Ok(CreateGithubCredentialResponse { credential_id })
}
