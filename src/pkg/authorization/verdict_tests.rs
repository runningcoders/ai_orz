//! 快照裁决纯函数单元测试（evaluate_snapshot / clamp_ttl_secs / default_max_uses）
//!
//! 拆分自 `verdict.rs` 尾部 tests 模块（文件瘦身，211 → 77 行）。
//! 用 `#[path]` 保持 `mod tests` 层级，测试里 `use super::*` 仍可见私有项。
//!
//! 覆盖口径：状态机五态裁决（Granted/Revoked/Expired/Exhausted/不构成放行）、
//! 签名命中与否、ttl 边界、max_uses 幂等默认值。

use super::*;
use crate::pkg::authorization::AuthorizationGrant;

const SAMPLE_SIG: &str = "docker push registry.local/app:1.0";

fn snapshot(
    status: AuthorizationStatus,
    prefix: bool,
    expires_at_ms: i64,
    max_uses: Option<u32>,
    uses: u32,
) -> AuthorizationPolicySnapshot {
    AuthorizationPolicySnapshot {
        grant_id: "g1".into(),
        agent_id: "agent-a".into(),
        tool_id: "shell_exec".into(),
        command_signature: SAMPLE_SIG.into(),
        prefix_match: prefix,
        status,
        expires_at_ms,
        max_uses,
        uses,
    }
}

fn grant() -> AuthorizationGrant {
    AuthorizationGrant {
        grant_id: "g1".into(),
        authorization_id: "a1".into(),
        agent_id: "agent-a".into(),
        tool_id: "shell_exec".into(),
        command_signature: SAMPLE_SIG.into(),
        prefix_match: false,
        expires_at_ms: 10_000,
        max_uses: Some(1),
        uses: 0,
    }
}

#[test]
fn granted_on_active_within_ttl_and_uses() {
    let s = snapshot(AuthorizationStatus::Active, false, 10_000, Some(3), 1);
    assert_eq!(
        evaluate_snapshot(&s, SAMPLE_SIG, 5_000),
        SnapshotVerdict::Granted
    );
}

#[test]
fn signature_miss_is_inactive() {
    let s = snapshot(AuthorizationStatus::Active, false, 10_000, None, 0);
    assert_eq!(
        evaluate_snapshot(&s, "docker push registry.local/other:2.0", 5_000),
        SnapshotVerdict::Inactive
    );
}

#[test]
fn prefix_grant_matches_longer_command() {
    let mut s = snapshot(AuthorizationStatus::Active, true, 10_000, None, 0);
    s.command_signature = "docker push".into();
    assert_eq!(
        evaluate_snapshot(&s, "docker push registry.local/other:2.0", 5_000),
        SnapshotVerdict::Granted
    );
    assert_eq!(
        evaluate_snapshot(&s, "docker pushx registry.local/other:2.0", 5_000),
        SnapshotVerdict::Inactive
    );
}

#[test]
fn non_active_status_paths() {
    let cases = [
        (AuthorizationStatus::Revoked, SnapshotVerdict::Revoked),
        (AuthorizationStatus::Expired, SnapshotVerdict::Expired),
        (AuthorizationStatus::Pending, SnapshotVerdict::Inactive),
        (AuthorizationStatus::Rejected, SnapshotVerdict::Inactive),
        (AuthorizationStatus::Consumed, SnapshotVerdict::Inactive),
    ];
    for (status, expected) in cases {
        let s = snapshot(status, false, 10_000, None, 0);
        assert_eq!(
            evaluate_snapshot(&s, SAMPLE_SIG, 5_000),
            expected,
            "status={status}"
        );
    }
}

#[test]
fn ttl_boundary_is_expired() {
    let s = snapshot(AuthorizationStatus::Active, false, 1_000, None, 0);
    assert_eq!(
        evaluate_snapshot(&s, SAMPLE_SIG, 1_000),
        SnapshotVerdict::Expired
    );
    assert_eq!(
        evaluate_snapshot(&s, SAMPLE_SIG, 999),
        SnapshotVerdict::Granted
    );
}

#[test]
fn exhausted_when_uses_reach_max() {
    let s = snapshot(AuthorizationStatus::Active, false, 10_000, Some(1), 1);
    assert_eq!(
        evaluate_snapshot(&s, SAMPLE_SIG, 5_000),
        SnapshotVerdict::Exhausted
    );
}

#[test]
fn snapshot_from_grant_evaluates_same() {
    let g = grant();
    let s = AuthorizationPolicySnapshot::from_grant(&g, AuthorizationStatus::Active);
    assert_eq!(
        evaluate_snapshot(&s, SAMPLE_SIG, 5_000),
        SnapshotVerdict::Granted
    );
    assert!(s.remaining_uses().is_some());
}

#[test]
fn ttl_clamp_and_default_max_uses_conventions() {
    assert_eq!(clamp_ttl_secs(10), TTL_MIN_SECS);
    assert_eq!(clamp_ttl_secs(100_000), TTL_MAX_SECS);
    assert_eq!(clamp_ttl_secs(900), 900);
    assert_eq!(default_max_uses(true), None);
    assert_eq!(default_max_uses(false), Some(1));
    assert!(SnapshotVerdict::Granted.granted());
    assert!(!SnapshotVerdict::Exhausted.granted());
}
