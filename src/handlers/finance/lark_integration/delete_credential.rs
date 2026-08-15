//! Handler: DELETE /api/v1/finance/identity/lark/credentials/{id} - 删除飞书应用凭证
//!
//! 有渠道引用时 Domain 报 Conflict。

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{DeleteLarkCredentialRequest, DeleteLarkCredentialResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn delete_credential(
    ctx: RequestContext,
    params: DeleteLarkCredentialRequest,
) -> Result<DeleteLarkCredentialResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    domain()
        .identity_credential_manage()
        .delete_credential(ctx, &user_id, &params.id)
        .await?;

    Ok(DeleteLarkCredentialResponse { success: true })
}
