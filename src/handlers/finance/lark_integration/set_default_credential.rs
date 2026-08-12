//! Handler: POST /api/v1/finance/identity/lark/credentials/default - 设置默认飞书凭证
//!
//! lark_cli 工具身份优先取引用默认凭证的渠道；空 credential_id 表示取消默认。

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{SetDefaultLarkCredentialRequest, SetDefaultLarkCredentialResponse};
use common::error::{Result, bail_err};

#[generate_http_handler]
pub async fn set_default_credential(
    ctx: RequestContext,
    params: SetDefaultLarkCredentialRequest,
) -> Result<SetDefaultLarkCredentialResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    domain()
        .identity_credential_manage()
        .set_default_lark_credential(ctx, &user_id, &params.credential_id)
        .await?;

    Ok(SetDefaultLarkCredentialResponse { success: true })
}
