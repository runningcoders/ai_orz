//! Handler: DELETE /api/v1/finance/identity/tavily/credentials/{id} - 删除 Tavily 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{DeleteTavilyCredentialRequest, DeleteTavilyCredentialResponse};
use common::error::Result;

#[generate_http_handler]
pub async fn delete_credential(
    ctx: RequestContext,
    params: DeleteTavilyCredentialRequest,
) -> Result<DeleteTavilyCredentialResponse> {
    let user_id = ctx.uid();

    domain()
        .identity_credential_manage()
        .delete_credential(ctx, &user_id, &params.id)
        .await?;

    Ok(DeleteTavilyCredentialResponse { success: true })
}
