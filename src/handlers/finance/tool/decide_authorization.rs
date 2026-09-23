//! Handler: 工具授权单审批决策（UI 直批 / 聊天指令直批）
//!
//! 红线④（§14.3/§15.1 定稿）：decide 直批强制 user ctx；Agent ctx 仅放行聊天
//! 代呈通道（confirm_authorization_via_chat），domain 层结构性强制。
//! 聊天指令直批（ChatDirective）由渠道入站管线以消息归属人身份携带证据调用。

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AuthorizationDecisionRequest, AuthorizationDecisionResponse, EvidenceClassDto};
use common::error::{Result, bail_err};

use crate::pkg::RequestContext;
use crate::service::domain::finance::tool_authorization::status_to_dto;
use crate::service::domain::finance::{AuthorizationDecisionCmd, domain};

/// 审批决策：批准签发（scope 裁量）/ 拒绝落档
#[register_handler_tool(
    id = "decide_authorization",
    name = "Decide Authorization",
    description = "Decide a pending tool authorization request: Approve issues a scoped, TTL-bounded, use-counted grant; Reject archives the decision. UI direct approval requires a user context; chat-directive approval must carry an evidence message ID. Agents cannot decide directly - use confirm_authorization_via_chat with user evidence instead.",
    params = "common::api::AuthorizationDecisionRequest",
    tags = "admin,tool_management"
)]
#[generate_http_handler]
pub async fn decide_authorization(
    ctx: RequestContext,
    params: AuthorizationDecisionRequest,
) -> Result<AuthorizationDecisionResponse> {
    let approve = match params.decision.as_str() {
        "Approve" => true,
        "Reject" => false,
        other => bail_err!(
            InvalidRequest,
            "非法决策值 decision={other}（仅支持 Approve/Reject）"
        ),
    };
    // 聊天代呈通道仅限 Agent ctx 专用入口（user 直批面不开放，防代呈身份伪造）
    if matches!(&params.evidence_class, Some(EvidenceClassDto::ChatMediated)) {
        bail_err!(
            InvalidRequest,
            "聊天代呈通道仅限 Agent 上下文（请走 confirm_authorization_via_chat）"
        );
    }
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
        evidence_class: params.evidence_class,
        evidence_message_id: params.evidence_message_id,
        // 直批通道无代呈 Agent：DTO 传值不信任（mediator 由 ctx 注入，user ctx 无 Agent）
        mediator_agent_id: None,
    };
    let outcome = domain()
        .tool_authorization_manage()
        .decide_authorization(ctx, cmd)
        .await?;
    // 对外出口统一脱敏（口径对齐 tool-call-entries）
    Ok(crate::redact!(AuthorizationDecisionResponse {
        authorization_id: outcome.authorization_id,
        status: status_to_dto(outcome.status),
        grant_id: outcome.grant_id,
    })?)
}
