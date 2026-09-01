//! Handler: POST /api/v1/message-channels/{id}/test - Test message channel connectivity

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{TestMessageChannelConnectionRequest, TestMessageChannelConnectionResponse};
use common::error::{Result, bail_err, err};

/// Test connectivity to a message channel by sending a test notification
#[register_handler_tool(
    id = "test_message_channel_connection",
    name = "Test Channel Connection",
    description = "Test connectivity to a message channel by sending a test notification",
    params = "common::api::TestMessageChannelConnectionRequest"
)]
#[generate_http_handler]
pub async fn test_message_channel_connection(
    ctx: RequestContext,
    params: TestMessageChannelConnectionRequest,
) -> Result<TestMessageChannelConnectionResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let channel = domain()
        .message_channel_manage()
        .get_message_channel(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "MessageChannel {} not found", params.id))?;

    if channel.po.org_id != org_id || channel.po.user_id != user_id {
        bail_err!(NotFound, "MessageChannel {} not found", params.id);
    }

    let response = match domain()
        .message_channel_manage()
        .test_message_channel(ctx, &channel)
        .await
    {
        Ok(()) => TestMessageChannelConnectionResponse {
            success: true,
            error: None,
        },
        Err(e) => TestMessageChannelConnectionResponse {
            success: false,
            error: Some(e.to_string()),
        },
    };

    Ok(response)
}
