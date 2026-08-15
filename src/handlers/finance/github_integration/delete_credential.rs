//! Handler: DELETE /api/v1/finance/identity/github/credentials/{id} - 删除 GitHub 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{DeleteGithubCredentialRequest, DeleteGithubCredentialResponse};
use common::error::Result;

#[generate_http_handler]
pub async fn delete_credential(
    ctx: RequestContext,
    params: DeleteGithubCredentialRequest,
) -> Result<DeleteGithubCredentialResponse> {
    let user_id = ctx.uid();

    domain()
        .identity_credential_manage()
        .delete_credential(ctx, &user_id, &params.id)
        .await?;

    Ok(DeleteGithubCredentialResponse { success: true })
}
