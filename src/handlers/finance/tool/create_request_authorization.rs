//! Handler: 工具授权单主动申请（管理面·create_request）
//!
//! 纵深校验（方案 §四/§13.3 定稿）：申请命令原文必须经拦截侧同一裁决引擎
//! （shell_policy::evaluate）复核且命中 Confirm 级规则方可建单——「被拦命令
//! 申请授权」语义闭环；未命中任何阻断规则 = 非受限操作（Conflict）；
//! 仅命中审计规则 = 无需授权（InvalidRequest）；命中 Deny 级规则 = 结构性
//! 不可授权放行（InvalidRequest，红线①：Deny 永不解锁）。
//! agent_id 归属：Agent ctx 强制取 ctx.agent_id（防 Agent 代建他 Agent 授权单）；
//! user ctx 使用 params.agent_id（代指定）。

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateAuthorizationRequest, CreateAuthorizationResponse};
use common::error::{Result, bail_err, err};

use crate::config::get;
use crate::pkg::RequestContext;
use crate::pkg::authorization::CreateAuthorizationCmd;
use crate::pkg::authorization::command_signature;
use crate::pkg::paths;
use crate::pkg::policy::PolicyAction;
use crate::pkg::tool_registry::shell_policy::{self, ShellPolicyInput};
use crate::service::domain::finance::domain;
use crate::service::domain::finance::tool_authorization::status_to_dto;

/// 主动申请建单：口头事前授权登记等场景（命令原文必携，纵深校验后入待审批）
#[register_handler_tool(
    id = "create_request_authorization",
    name = "Create Request Authorization",
    description = "Create a pending tool authorization request for a restricted command on behalf of the caller. The raw command is deep-validated by the same shell policy engine as the interception layer: only Confirm-level hits are accepted as restricted operations; unrestricted or audit-only commands are rejected and Deny-level commands are never approvable. Agent contexts are force-scoped to their own agent_id; user contexts must specify the target agent.",
    params = "common::api::CreateAuthorizationRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn create_request_authorization(
    ctx: RequestContext,
    params: CreateAuthorizationRequest,
) -> Result<CreateAuthorizationResponse> {
    // ① 命令原文必携（签名计算与纵深校验依据）
    let command = params
        .command
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "主动建单必须携带受限命令原文 command"))?;

    // ② 纵深校验：与拦截侧同一引擎；工作目录按调用身份默认解析（无显式
    //    working_dir 的执行时默认形态一致），白名单为空
    let base_path = get().base_data_path();
    let base_root = base_path.to_string_lossy().into_owned();
    let working_dir =
        paths::default_workspace(&base_path, ctx.user_id.as_deref(), ctx.agent_id.as_deref());
    let verdict = shell_policy::evaluate(ShellPolicyInput {
        command: &command,
        working_dir: &working_dir,
        base_root: &base_root,
        additional_allowed_paths: &[],
        user_id: ctx.user_id.as_deref(),
        agent_id: ctx.agent_id.as_deref(),
    });
    // 裁决分支：Confirm=受限操作放行建单；Deny=结构性不可授权；Audit=无需授权；
    // 未命中=非受限操作（注意：Audit-only 命中时 blocking=None、audits 非空）
    let (rule_id, rule_idempotent) = match &verdict.blocking {
        Some(PolicyAction::Confirm(_)) => {
            let rule_id = verdict.blocking_rule.unwrap_or_default();
            let idempotent = shell_policy::rule_def(rule_id)
                .map(|d| d.idempotent)
                .unwrap_or(false);
            (rule_id.to_string(), idempotent)
        }
        Some(PolicyAction::Deny(_)) => bail_err!(
            InvalidRequest,
            "申请命令命中 Deny 级规则（{}），结构性不可授权放行",
            verdict.blocking_rule.unwrap_or_default()
        ),
        _ if !verdict.audits.is_empty() => bail_err!(
            InvalidRequest,
            "申请行为仅命中审计规则（{}）非受限操作，无需授权",
            verdict.audits[0].0
        ),
        _ => bail_err!(Conflict, "申请行为非受限操作（未命中任何拦截规则）"),
    };

    // ③ agent_id 归属：Agent ctx 强制取自身，不信任 params 传值（防代建）
    let agent_id = match ctx.agent_id.clone() {
        Some(aid) => aid,
        None => params.agent_id,
    };

    let cmd = CreateAuthorizationCmd {
        agent_id,
        tool_id: params.tool_id,
        command_signature: command_signature(&command),
        blocking_rule: rule_id,
        rule_idempotent,
        reason: Some(params.reason),
        // 主动建单没有「被拦的调用」上下文：call_id 恒 None（不能填本次
        // create_request_authorization 自己的调用 ID，那会把审批单错挂到本工具上）
        call_id: None,
    };
    let pending = domain()
        .tool_authorization_manage()
        .create_pending_authorization(ctx, cmd)
        .await?;
    // 对外出口统一脱敏（口径对齐 tool-call-entries）
    Ok(crate::redact!(CreateAuthorizationResponse {
        authorization_id: pending.authorization_id,
        status: status_to_dto(pending.status),
    })?)
}
