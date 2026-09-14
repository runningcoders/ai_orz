//! Handler: POST /api/v1/finance/identity/email/credentials - 创建邮箱机器人凭证

use crate::pkg::RequestContext;
use crate::service::domain::finance::{CreateCredentialCmd, domain};
use ai_orz_macros::generate_http_handler;
use common::api::{CreateEmailBotCredentialRequest, CreateEmailBotCredentialResponse};
use common::error::{Result, bail_err};
use common::models::CredentialDetail;

#[generate_http_handler]
pub async fn create_credential(
    ctx: RequestContext,
    params: CreateEmailBotCredentialRequest,
) -> Result<CreateEmailBotCredentialResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let platform = params.platform.trim().to_string();
    if platform.is_empty() {
        bail_err!(InvalidRequest, "platform 不能为空");
    }

    let credential_id = domain()
        .identity_credential_manage()
        .create_credential(
            ctx,
            &user_id,
            CreateCredentialCmd {
                name: params.name,
                detail: CredentialDetail::EmailBot {
                    email_address: params.email_address,
                    smtp_host: params.smtp_host,
                    smtp_port: params.smtp_port,
                    imap_host: params.imap_host,
                    imap_port: params.imap_port,
                    username: params.username,
                    password: params.password,
                },
                platform: Some(platform),
            },
        )
        .await?;

    Ok(CreateEmailBotCredentialResponse { credential_id })
}
