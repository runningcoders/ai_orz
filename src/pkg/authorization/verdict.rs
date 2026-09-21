//! 快照裁决纯函数与签发约定（pkg/authorization 基建）
//!
//! 裁决为纯判定：不落库、不通知、不加锁——Gate/dal 消费方据此完成
//! 层级推导（Granted 降级 Audit 放行 + consume；未命中短路建单）。

use super::model::{AuthorizationPolicySnapshot, AuthorizationStatus};
use super::signing::signature_matches;

/// 快照裁决结论
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotVerdict {
    /// 命中有效授权：降级 Audit 放行 + consume 计数
    Granted,
    /// 已撤销（即时生效）
    Revoked,
    /// 已过期（状态或 ttl 到期）
    Expired,
    /// 次数用尽
    Exhausted,
    /// 不构成放行（签名未命中 / Pending / Rejected / Consumed 等）
    Inactive,
}

impl SnapshotVerdict {
    pub fn granted(&self) -> bool {
        matches!(self, SnapshotVerdict::Granted)
    }
}

/// 快照裁决：签名命中 → 状态/ttl/次数逐层判定。
/// 身份维度 (agent_id, tool_id) 过滤由上游（domain 索引或 dal precheck 调用方）完成。
pub fn evaluate_snapshot(
    snapshot: &AuthorizationPolicySnapshot,
    command_signature: &str,
    now_ms: i64,
) -> SnapshotVerdict {
    if !signature_matches(
        &snapshot.command_signature,
        snapshot.prefix_match,
        command_signature,
    ) {
        return SnapshotVerdict::Inactive;
    }
    match snapshot.status {
        AuthorizationStatus::Active => {}
        AuthorizationStatus::Revoked => return SnapshotVerdict::Revoked,
        AuthorizationStatus::Expired => return SnapshotVerdict::Expired,
        AuthorizationStatus::Pending
        | AuthorizationStatus::Rejected
        | AuthorizationStatus::Consumed => return SnapshotVerdict::Inactive,
    }
    if now_ms >= snapshot.expires_at_ms {
        return SnapshotVerdict::Expired;
    }
    if snapshot.max_uses.is_some_and(|max| snapshot.uses >= max) {
        return SnapshotVerdict::Exhausted;
    }
    SnapshotVerdict::Granted
}

/// 审批签发 ttl 约定（方案 §四：clamp [60,3600]s，默认 900s）
pub const TTL_MIN_SECS: i64 = 60;
pub const TTL_MAX_SECS: i64 = 3600;
pub const DEFAULT_TTL_SECS: i64 = 900;

pub fn clamp_ttl_secs(secs: i64) -> i64 {
    secs.clamp(TTL_MIN_SECS, TTL_MAX_SECS)
}

/// 规则幂等属性 → 签发默认次数上限（§14.2 定案）：
/// 非幂等规则默认仅放行 1 次；幂等规则不设次数上限（ttl 内有效），Scope 可显式覆盖。
pub fn default_max_uses(idempotent: bool) -> Option<u32> {
    if idempotent { None } else { Some(1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::authorization::{AuthorizationGrant, command_signature};

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
}
