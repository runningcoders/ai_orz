//! Handler: DELETE /api/v1/finance/identity/generic-token/credentials/{id} - 删除通用 API Token 凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{DeleteGenericTokenCredentialRequest, DeleteGenericTokenCredentialResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn delete_credential(
    ctx: RequestContext,
    params: DeleteGenericTokenCredentialRequest,
) -> Result<DeleteGenericTokenCredentialResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    domain()
        .identity_credential_manage()
        .delete_credential(ctx, &user_id, &params.id)
        .await?;

    Ok(DeleteGenericTokenCredentialResponse { success: true })
}
