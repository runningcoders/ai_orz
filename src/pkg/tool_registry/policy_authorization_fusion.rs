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
mod tests {
    use super::*;
    use crate::pkg::authorization::authorization_gate::AuthorizationGate;
    use crate::pkg::authorization::{AuthorizationGrant, PendingAuthorization};
    use async_trait::async_trait;
    use common::error::bail_err;
    use std::collections::HashMap;
    use std::sync::Mutex;

    const SIG: &str = "docker push registry.local/app:1.0";

    /// 可控 FakeGate：注入授权状态驱动裁决矩阵各分支
    struct FakeGate {
        grants: Mutex<HashMap<String, AuthorizationGrant>>,
        /// request 调用计数（建单/复用分支验证）
        requests: std::sync::atomic::AtomicUsize,
        /// request 返回错误（模拟同签名 Pending 已存在 → 复用分支）
        fail_requests: std::sync::atomic::AtomicBool,
    }

    impl FakeGate {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                grants: Mutex::new(HashMap::new()),
                requests: std::sync::atomic::AtomicUsize::new(0),
                fail_requests: std::sync::atomic::AtomicBool::new(false),
            })
        }

        fn add_grant(&self, auth_id: &str, grant_id: &str, max_uses: Option<u32>) {
            let g = AuthorizationGrant {
                grant_id: grant_id.to_string(),
                authorization_id: auth_id.to_string(),
                agent_id: "agent-a".to_string(),
                tool_id: "shell".to_string(),
                command_signature: SIG.to_string(),
                prefix_match: false,
                expires_at_ms: i64::MAX,
                max_uses,
                uses: 0,
            };
            self.grants.lock().unwrap().insert(auth_id.to_string(), g);
        }
    }

    #[async_trait]
    impl AuthorizationGate for FakeGate {
        async fn check_grant(
            &self,
            _ctx: RequestContext,
            agent_id: &str,
            tool_id: &str,
            command_signature: &str,
        ) -> Result<Vec<AuthorizationGrant>> {
            assert_eq!(agent_id, "agent-a");
            assert_eq!(tool_id, "shell");
            Ok(self
                .grants
                .lock()
                .unwrap()
                .values()
                .filter(|g| {
                    crate::pkg::authorization::signature_matches(
                        &g.command_signature,
                        g.prefix_match,
                        command_signature,
                    )
                })
                .cloned()
                .collect())
        }

        async fn request_authorization(
            &self,
            _ctx: RequestContext,
            cmd: crate::service::domain::finance::CreateAuthorizationCmd,
        ) -> Result<PendingAuthorization> {
            self.requests
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.fail_requests.load(std::sync::atomic::Ordering::SeqCst) {
                bail_err!(Conflict, "同签名待审批授权单已存在 authorization_id=x");
            }
            Ok(PendingAuthorization {
                authorization_id: "auth-new".to_string(),
                agent_id: cmd.agent_id,
                tool_id: cmd.tool_id,
                user_id: "user-aman".to_string(),
                command_signature: cmd.command_signature,
                blocking_rule: cmd.blocking_rule,
                requested_at_ms: 0,
                status: crate::pkg::authorization::AuthorizationStatus::Pending,
            })
        }

        async fn consume(
            &self,
            _ctx: RequestContext,
            authorization_id: &str,
        ) -> Result<Option<AuthorizationGrant>> {
            let mut grants = self.grants.lock().unwrap();
            let Some(g) = grants.get_mut(authorization_id) else {
                return Ok(None);
            };
            if g.max_uses.is_some_and(|max| g.uses >= max) {
                return Ok(None);
            }
            g.uses += 1;
            Ok(Some(g.clone()))
        }
    }

    async fn ctx_with_storage() -> RequestContext {
        // RequestContext::builder().build() 需要 storage::get()；测试先初始化全局（幂等）
        crate::pkg::storage::test_support::init_for_test().await;
        RequestContext::builder()
            .user_id("user-aman")
            .agent_id("agent-a")
            .build()
    }

    async fn run_confirm(gate: Option<&Arc<dyn AuthorizationGate>>) -> ConfirmOutcome {
        let ctx = ctx_with_storage().await;
        confirm_with_authorization(
            &ctx,
            gate,
            "git_dangerous_subcommand",
            "blocked".to_string(),
            SIG,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn confirm_without_gate_keeps_current_behavior() {
        let out = run_confirm(None).await;
        match out {
            ConfirmOutcome::Pending { created, .. } => assert!(!created),
            _ => panic!("expected Pending"),
        }
    }

    #[tokio::test]
    async fn confirm_with_valid_grant_releases_and_consumes() {
        let gate = FakeGate::new();
        gate.add_grant("auth-1", "g-1", Some(2));
        let dyn_gate: Arc<dyn AuthorizationGate> = gate.clone();
        let out = run_confirm(Some(&dyn_gate)).await;
        match out {
            ConfirmOutcome::Released(r) => {
                assert_eq!(r.authorization_id, "auth-1");
                assert_eq!(r.remaining_uses, Some(1));
            }
            _ => panic!("expected Released"),
        }
        // 再次放行到耗尽
        let out = run_confirm(Some(&dyn_gate)).await;
        match out {
            ConfirmOutcome::Released(r) => assert_eq!(r.remaining_uses, Some(0)),
            _ => panic!("expected Released second"),
        }
        // 第三次：耗尽 → 建单
        let out = run_confirm(Some(&dyn_gate)).await;
        match out {
            ConfirmOutcome::Pending { created, .. } => assert!(created),
            _ => panic!("expected Pending after exhaust"),
        }
    }

    #[tokio::test]
    async fn confirm_without_grant_creates_pending() {
        let gate = FakeGate::new();
        let dyn_gate: Arc<dyn AuthorizationGate> = gate.clone();
        let out = run_confirm(Some(&dyn_gate)).await;
        match out {
            ConfirmOutcome::Pending {
                pending_id,
                created,
                ..
            } => {
                assert!(created);
                assert_eq!(pending_id, "auth-new");
            }
            _ => panic!("expected Pending"),
        }
        assert_eq!(gate.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn confirm_with_existing_pending_reuses() {
        let gate = FakeGate::new();
        gate.fail_requests
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let dyn_gate: Arc<dyn AuthorizationGate> = gate.clone();
        let out = run_confirm(Some(&dyn_gate)).await;
        match out {
            ConfirmOutcome::Pending { created, .. } => assert!(!created),
            _ => panic!("expected Pending reuse"),
        }
    }

    #[test]
    fn blocking_response_includes_pending_fields() {
        let body = blocking_response("blocked", Some("auth-1"));
        assert_eq!(body["authorization_id"], "auth-1");
        assert_eq!(body["pending_approval"], true);
        let body = blocking_response("blocked", None);
        assert!(body.get("authorization_id").is_none());
    }
}
