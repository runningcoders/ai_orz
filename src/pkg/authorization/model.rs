//! 授权单领域模型与六态枚举（pkg/authorization 基建 · 纯值类型）

use std::fmt;

/// 授权单六态领域枚举（方案 §四/§15.2 定稿）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorizationStatus {
    /// 待审批（拦截建单初始态）
    Pending,
    /// 已批准生效（Grant 可用）
    Active,
    /// 已过期（ttl 到期，domain 惰性判定）
    Expired,
    /// 已撤销（即时生效）
    Revoked,
    /// 已拒绝
    Rejected,
    /// 已消耗（非幂等单次放行后落档终态）
    Consumed,
}

impl AuthorizationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthorizationStatus::Pending => "Pending",
            AuthorizationStatus::Active => "Active",
            AuthorizationStatus::Expired => "Expired",
            AuthorizationStatus::Revoked => "Revoked",
            AuthorizationStatus::Rejected => "Rejected",
            AuthorizationStatus::Consumed => "Consumed",
        }
    }

    /// 终态标注（六态流转白名单由 domain 审批状态机维护，基建只标注终态）
    pub fn terminal(&self) -> bool {
        matches!(
            self,
            AuthorizationStatus::Expired
                | AuthorizationStatus::Revoked
                | AuthorizationStatus::Rejected
                | AuthorizationStatus::Consumed
        )
    }
}

impl fmt::Display for AuthorizationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 待审批授权单（拦截建单态）
///
/// 字段直白命名，Agent×工具粒度本期不泛化（§15.4 YAGNI 定案）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAuthorization {
    pub authorization_id: String,
    /// 申请人 Agent（红线⑤：审批校验代呈 Agent 不得等于申请人）
    pub agent_id: String,
    pub tool_id: String,
    /// 归属用户（审批人；用户消息不可伪造，证据链以此为准）
    pub user_id: String,
    /// 命中受限命令的规范化签名（同签名重试放行与授权查表键）
    pub command_signature: String,
    /// 命中规则 id（审计 released_rule；白名单按 rule_id 判定而非 reason 文案）
    pub blocking_rule: String,
    /// 建单时间 ms（证据五要素第④条「证据晚于建单」的基准）
    pub requested_at_ms: i64,
    /// 当前状态（六态流转由 domain 审批状态机维护）
    pub status: AuthorizationStatus,
}

/// 已签发授权（Approve 后生效）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationGrant {
    pub grant_id: String,
    pub authorization_id: String,
    pub agent_id: String,
    pub tool_id: String,
    /// 授权命令签名（默认精确匹配=最窄授权）
    pub command_signature: String,
    /// true=前缀匹配放宽（Approve 可裁量 scope）
    pub prefix_match: bool,
    /// 过期时刻 ms（绝对时间）
    pub expires_at_ms: i64,
    /// 次数上限（None=ttl 内不限次；非幂等规则默认 1 次）
    pub max_uses: Option<u32>,
    /// 已放行消耗次数
    pub uses: u32,
}

impl AuthorizationGrant {
    pub fn remaining_uses(&self) -> Option<u32> {
        self.max_uses.map(|max| max.saturating_sub(self.uses))
    }
}

/// 授权策略快照：授权记录在裁决瞬间的只读投影（§15.2 上移基建；
/// 两段化时经参数流入 dal 用完即弃，不缓存不持有）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationPolicySnapshot {
    pub grant_id: String,
    pub agent_id: String,
    pub tool_id: String,
    pub command_signature: String,
    pub prefix_match: bool,
    pub status: AuthorizationStatus,
    pub expires_at_ms: i64,
    pub max_uses: Option<u32>,
    pub uses: u32,
}

impl AuthorizationPolicySnapshot {
    /// 从 Grant 生成快照（domain 侧组装）
    pub fn from_grant(grant: &AuthorizationGrant, status: AuthorizationStatus) -> Self {
        Self {
            grant_id: grant.grant_id.clone(),
            agent_id: grant.agent_id.clone(),
            tool_id: grant.tool_id.clone(),
            command_signature: grant.command_signature.clone(),
            prefix_match: grant.prefix_match,
            status,
            expires_at_ms: grant.expires_at_ms,
            max_uses: grant.max_uses,
            uses: grant.uses,
        }
    }

    /// 身份匹配：快照是否适用于 (agent_id, tool_id) 维度
    pub fn applies_to(&self, agent_id: &str, tool_id: &str) -> bool {
        self.agent_id == agent_id && self.tool_id == tool_id
    }

    pub fn remaining_uses(&self) -> Option<u32> {
        self.max_uses.map(|max| max.saturating_sub(self.uses))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SIG: &str = "docker push registry.local/app:1.0";

    fn sample_grant() -> AuthorizationGrant {
        AuthorizationGrant {
            grant_id: "g1".into(),
            authorization_id: "a1".into(),
            agent_id: "agent-a".into(),
            tool_id: "shell_exec".into(),
            command_signature: SAMPLE_SIG.into(),
            prefix_match: false,
            expires_at_ms: 1_000,
            max_uses: Some(1),
            uses: 0,
        }
    }

    #[test]
    fn status_str_and_terminal_flags() {
        assert_eq!(AuthorizationStatus::Pending.as_str(), "Pending");
        assert_eq!(AuthorizationStatus::Active.to_string(), "Active");
        assert!(!AuthorizationStatus::Active.terminal());
        assert!(!AuthorizationStatus::Pending.terminal());
        assert!(AuthorizationStatus::Expired.terminal());
        assert!(AuthorizationStatus::Revoked.terminal());
        assert!(AuthorizationStatus::Rejected.terminal());
        assert!(AuthorizationStatus::Consumed.terminal());
    }

    #[test]
    fn snapshot_from_grant_and_identity_match() {
        let grant = sample_grant();
        let snap = AuthorizationPolicySnapshot::from_grant(&grant, AuthorizationStatus::Active);
        assert!(snap.applies_to("agent-a", "shell_exec"));
        assert!(!snap.applies_to("agent-b", "shell_exec"));
        assert!(!snap.applies_to("agent-a", "shell_tool"));
        assert_eq!(snap.remaining_uses(), Some(1));
    }

    #[test]
    fn remaining_uses_saturates_and_none_means_unlimited() {
        let mut grant = sample_grant();
        grant.uses = 5;
        assert_eq!(grant.remaining_uses(), Some(0));
        grant.max_uses = None;
        assert_eq!(grant.remaining_uses(), None);
    }
}
