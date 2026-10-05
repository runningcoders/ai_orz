//! 拦截层 × 授权门融合（方案 §七/§八 裁决矩阵；阶段③批次一 S2）
//!
//! 职责：对 shell 域两执行入口统一收口「policy 裁决 → 授权层级推导 → 建单/放行」。
//! 设计约束（红线六条）：
//! ① Deny 结构性不可解锁——授权策略物理上不进 Deny 分支；
//! ② Confirm 全量经授权门层级推导（Granted 降级 Audit 放行+consume；未命中建单短路）；
//! ③ Audit 与未命中场景保持现状（默认无授权单时行为与现状逐字节一致）；
//! ⑥ 放行/建单全程审计留痕（log_info）。
//!
//! gate 作参数传入（非全局态直取）：调用点仅 shell 两入口，
//! 未装配 gate（None）时保持现状行为，兼容性由调用点保证。

use crate::pkg::RequestContext;
use crate::pkg::authorization::authorization_gate::AuthorizationGate;
use crate::pkg::authorization::{AuthorizationPolicySnapshot, evaluate_snapshot};
use crate::pkg::tool_registry::shell_policy::{ShellPolicyVerdict, rule_def};
use common::error::Result;
use serde_json::json;
use std::sync::Arc;

/// 授权放行消耗的 Grant（含授权单 ID，供调用点审计）
pub struct GrantedRelease {
    pub authorization_id: String,
    pub grant_id: String,
    pub remaining_uses: Option<u32>,
}

/// Confirm 阻断时授权融合结果
pub enum ConfirmOutcome {
    /// 已有有效授权 → 降级 Audit 放行（消耗一次）
    Released(GrantedRelease),
    /// 无有效授权 → 建单/复用待审批单，短路返回
    Pending {
        pending_id: String,
        /// true=新建 / false=复用既有 Pending（防通知风暴）
        created: bool,
        blocked_by: &'static str,
        reason: String,
    },
}

/// 融合判定（Confirm 分支专用；Deny/Audit/未命中由调用点直接走现状路径）
///
/// 返回 Err 仅在 gate 内部错误（存储异常等）——此场景保守短路不执行命令。
pub async fn confirm_with_authorization(
    ctx: &RequestContext,
    gate: Option<&Arc<dyn AuthorizationGate>>,
    blocked_by: &'static str,
    reason: String,
    command_signature: &str,
) -> Result<ConfirmOutcome> {
    let Some(gate) = gate else {
        // 未装配授权门（测试环境）→ 现状行为：短路返回
        return Ok(ConfirmOutcome::Pending {
            pending_id: String::new(),
            created: false,
            blocked_by,
            reason,
        });
    };
    let agent_id = ctx.agent_id.clone().unwrap_or_default();
    let tool_id = "shell";

    // 1) 查有效授权（gate 实现按签名匹配过滤）
    let grants = gate
        .check_grant(ctx.clone(), &agent_id, tool_id, command_signature)
        .await?;

    // 2) 层级推导：快照裁决取首个 Granted
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    for grant in &grants {
        let snapshot = AuthorizationPolicySnapshot::from_grant(
            grant,
            crate::pkg::authorization::AuthorizationStatus::Active,
        );
        if matches!(
            evaluate_snapshot(&snapshot, command_signature, now_ms),
            crate::pkg::authorization::SnapshotVerdict::Granted
        ) {
            let consumed = gate.consume(ctx.clone(), &grant.authorization_id).await?;
            if consumed.is_some() {
                crate::log_info!(
                    "授权放行 [{}|{}] authorization_id={} grant_id={}",
                    agent_id,
                    tool_id,
                    grant.authorization_id,
                    grant.grant_id
                );
                return Ok(ConfirmOutcome::Released(GrantedRelease {
                    authorization_id: grant.authorization_id.clone(),
                    grant_id: grant.grant_id.clone(),
                    remaining_uses: consumed.and_then(|g| g.remaining_uses()),
                }));
            }
            // consume 返回 None（并发耗尽/失效）→ 继续看下一授权或建单
        }
    }

    // 3) 未命中 → 建单（同签名 Pending 已存在时 gate 实现拒绝，转复用语义）
    let cmd = crate::pkg::authorization::CreateAuthorizationCmd {
        agent_id: agent_id.clone(),
        tool_id: tool_id.to_string(),
        command_signature: command_signature.to_string(),
        blocking_rule: blocked_by.to_string(),
        rule_idempotent: rule_def(blocked_by).map(|r| r.idempotent).unwrap_or(false),
        reason: Some(reason.clone()),
        // 触发拦截的那次工具调用（trace call_id 同一事实源）⇒ 授权单可反查调用记录
        call_id: ctx.tool_call_id().cloned(),
    };
    match gate.request_authorization(ctx.clone(), cmd).await {
        Ok(pending) => {
            crate::log_info!(
                "授权建单 [{}|{}] authorization_id={} rule={}",
                agent_id,
                tool_id,
                pending.authorization_id,
                blocked_by
            );
            Ok(ConfirmOutcome::Pending {
                pending_id: pending.authorization_id,
                created: true,
                blocked_by,
                reason,
            })
        }
        Err(_) => {
            // 已存在同签名 Pending → 查取复用（check 不返回 Pending，这里尽力补拿）
            // 简化：直接以现有行为短路返回，前端提示等待审批
            crate::log_info!(
                "授权待审批复用 [{}|{}] rule={}",
                agent_id,
                tool_id,
                blocked_by
            );
            Ok(ConfirmOutcome::Pending {
                pending_id: String::new(),
                created: false,
                blocked_by,
                reason,
            })
        }
    }
}

/// 统一短路返回体（拦截/建单/待复用共用；方案 §八 返回体扩展 authorization_id）
pub fn blocking_response(reason: &str, pending_id: Option<&str>) -> serde_json::Value {
    let mut body = json!({
        "success": false,
        "require_confirmation": true,
        "error": reason,
        "message": reason,
    });
    if pending_id.is_some_and(|id| !id.is_empty()) {
        let id = pending_id.unwrap();
        body["authorization_id"] = json!(id);
        body["pending_approval"] = json!(true);
    }
    body
}

/// 判定 verdict 中 Confirm 场景的规则幂等性（建单 cmd 用；诊断辅助保留）
#[allow(dead_code)]
pub fn blocked_rule_idempotent(verdict: &ShellPolicyVerdict) -> bool {
    verdict
        .blocking_rule
        .and_then(rule_def)
        .map(|r| r.idempotent)
        .unwrap_or(false)
}
#[cfg(test)]
#[path = "policy_authorization_fusion_tests.rs"]
mod tests;
