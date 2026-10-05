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
#[path = "verdict_tests.rs"]
mod tests;
