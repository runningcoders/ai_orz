//! Handler: 聊天确认代呈（Agent 携用户证据消息代呈审批决策）
//!
//! 红线⑤（§15.1 定稿）：Agent 仅代呈不代决策——必须携带用户明确表态的
//! 证据消息 ID（平台五要素校验：消息归属审批用户、来源为用户、晚于建单）；
//! 申请人不得自代呈；代呈 Agent 身份由 ctx 强制注入，不信任客户端传值。

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AuthorizationDecisionRequest, AuthorizationDecisionResponse, EvidenceClassDto};
use common::error::{Result, bail_err, err};

use crate::pkg::RequestContext;
use crate::service::domain::finance::tool_authorization::status_to_dto;
use crate::service::domain::finance::{AuthorizationDecisionCmd, domain};

/// 聊天确认代呈：将用户在聊天中的明确决策转呈平台（支持批准与拒绝）
#[register_handler_tool(
    id = "confirm_authorization_via_chat",
    name = "Confirm Authorization Via Chat",
    description = "Relay the user's explicit chat decision on a pending tool authorization. Call ONLY after the user clearly approved or rejected in chat, and pass the evidence message ID of the user's own message. The platform verifies the evidence chain (message owner = authorizing user, sent by the user, fresh, applicant cannot self-mediate). You relay the decision - you never make it.",
    params = "common::api::AuthorizationDecisionRequest",
    tags = "messaging,tool_management"
)]
#[generate_http_handler]
pub async fn confirm_authorization_via_chat(
    ctx: RequestContext,
    params: AuthorizationDecisionRequest,
) -> Result<AuthorizationDecisionResponse> {
    // 仅 Agent 上下文可代呈（用户直批走 decide_authorization）
    let agent_id = ctx.agent_id.clone().ok_or_else(|| {
        err!(
            InvalidRequest,
            "聊天代呈仅限 Agent 上下文（用户直批请走 decide_authorization）"
        )
    })?;
    if params.evidence_message_id.is_none() {
        bail_err!(
            InvalidRequest,
            "代呈必须携带证据消息 ID（用户明确表态的原话消息）"
        );
    }
    let approve = match params.decision.as_str() {
        "Approve" => true,
        "Reject" => false,
        other => bail_err!(
            InvalidRequest,
            "非法决策值 decision={other}（仅支持 Approve/Reject）"
        ),
    };
    // 证据类别与代呈 Agent 由 handler 从 ctx 强制注入（不信任客户端传值）
    let cmd = AuthorizationDecisionCmd {
        authorization_id: params.authorization_id,
        approve,
        scope_command_signature: params
            .scope
            .as_ref()
            .and_then(|s| s.command_signature.clone()),
        prefix_match: params.scope.as_ref().is_some_and(|s| s.prefix_match),
        max_uses: params.scope.as_ref().and_then(|s| s.max_uses),
        ttl_secs: params.scope.as_ref().and_then(|s| s.ttl_secs),
        evidence_class: Some(EvidenceClassDto::ChatMediated),
        evidence_message_id: params.evidence_message_id,
        mediator_agent_id: Some(agent_id),
    };
    let outcome = domain()
        .tool_authorization_manage()
        .decide_authorization(ctx, cmd)
        .await?;
    Ok(AuthorizationDecisionResponse {
        authorization_id: outcome.authorization_id,
        status: status_to_dto(outcome.status),
        grant_id: outcome.grant_id,
    })
}
