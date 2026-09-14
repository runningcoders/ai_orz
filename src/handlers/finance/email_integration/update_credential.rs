//! Handler: PATCH /api/v1/finance/identity/email/credentials/{id} - 更新邮箱机器人凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::{UpdateCredentialCmd, domain};
use ai_orz_macros::generate_http_handler;
use common::api::{UpdateEmailBotCredentialRequest, UpdateEmailBotCredentialResponse};
use common::error::{Result, bail_err};
use common::models::CredentialDetailPatch;

#[generate_http_handler]
pub async fn update_credential(
    ctx: RequestContext,
    params: UpdateEmailBotCredentialRequest,
) -> Result<UpdateEmailBotCredentialResponse> {
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
                patch: CredentialDetailPatch::EmailBot {
                    email_address: params.email_address,
                    smtp_host: params.smtp_host,
                    smtp_port: params.smtp_port,
                    imap_host: params.imap_host,
                    imap_port: params.imap_port,
                    username: params.username,
                    password: params.password,
                },
            },
        )
        .await?;

    Ok(UpdateEmailBotCredentialResponse { success: true })
}
