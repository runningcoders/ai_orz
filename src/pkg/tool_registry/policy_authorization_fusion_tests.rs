//! tests 单元测试（拆分自 policy_authorization_fusion.rs）
//!
//! 文件瘦身：原 385 行 → 177 行，测试体 209 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

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
            call_id: cmd.call_id,
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
