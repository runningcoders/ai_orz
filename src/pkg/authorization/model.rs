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

/// 用户证据静态四要素载体（第⑤条自代呈为独立参数比较；单次消费防重放由 domain 状态承载）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceCheck {
    /// 证据消息发送者 ID（必须等于授权单归属用户）
    pub from_id: String,
    /// 发送者角色是否为用户（伪造用户消息结构性拒绝）
    pub from_role_is_user: bool,
    /// 消息创建时刻 ms（必须晚于授权单建单时刻）
    pub created_at_ms: i64,
}

/// 证据校验拒绝原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceRejection {
    /// 未携带证据
    Missing,
    /// 证据消息归属用户与授权单归属用户不一致
    MismatchedUser,
    /// 证据消息发送者角色非用户
    NotFromUser,
    /// 证据消息早于建单时刻（先斩后奏）
    Stale,
    /// 代呈 Agent 即申请人（红线⑤：申请人不得自代呈）
    SelfMediation,
}

impl EvidenceRejection {
    pub fn describe(&self) -> String {
        match self {
            EvidenceRejection::Missing => "未携带用户证据".to_string(),
            EvidenceRejection::MismatchedUser => {
                "证据消息归属用户与授权单归属用户不一致".to_string()
            }
            EvidenceRejection::NotFromUser => "证据消息发送者角色非用户".to_string(),
            EvidenceRejection::Stale => "证据消息早于授权单建单时刻".to_string(),
            EvidenceRejection::SelfMediation => {
                "申请人不得自代呈（代呈 Agent 即申请人）".to_string()
            }
        }
    }
}

/// 证据静态校验纯函数（证据五要素：①存在 ②归属用户 ③角色为用户 ④晚于建单 ⑤代呈非申请人）
///
/// 返回 Ok(()) 表示静态要素齐备；单次消费防重放与「消息真实存在」的落库一致性
/// 由消费方（domain：查消息表 + evidence 消费记录）保证。
pub fn evaluate_evidence(
    evidence: Option<&EvidenceCheck>,
    authorization: &PendingAuthorization,
    mediator_agent_id: Option<&str>,
) -> Result<(), EvidenceRejection> {
    if mediator_agent_id.is_some_and(|m| m == authorization.agent_id) {
        return Err(EvidenceRejection::SelfMediation);
    }
    let ev = evidence.ok_or(EvidenceRejection::Missing)?;
    if ev.from_id != authorization.user_id {
        return Err(EvidenceRejection::MismatchedUser);
    }
    if !ev.from_role_is_user {
        return Err(EvidenceRejection::NotFromUser);
    }
    if ev.created_at_ms < authorization.requested_at_ms {
        return Err(EvidenceRejection::Stale);
    }
    Ok(())
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
    fn sample_pending() -> PendingAuthorization {
        PendingAuthorization {
            authorization_id: "a1".into(),
            agent_id: "agent-a".into(),
            tool_id: "shell_exec".into(),
            user_id: "user-aman".into(),
            command_signature: SAMPLE_SIG.into(),
            blocking_rule: "git_dangerous_subcommand".into(),
            requested_at_ms: 1_000,
            status: AuthorizationStatus::Pending,
        }
    }

    fn evidence(uid: &str, is_user: bool, at_ms: i64) -> EvidenceCheck {
        EvidenceCheck {
            from_id: uid.into(),
            from_role_is_user: is_user,
            created_at_ms: at_ms,
        }
    }

    #[test]
    fn evidence_passes_when_all_static_elements_hold() {
        let pending = sample_pending();
        let ev = evidence("user-aman", true, 2_000);
        assert!(evaluate_evidence(Some(&ev), &pending, Some("agent-b")).is_ok());
        assert!(evaluate_evidence(Some(&ev), &pending, None).is_ok());
    }

    #[test]
    fn evidence_rejections_cover_all_paths() {
        let pending = sample_pending();
        assert_eq!(
            evaluate_evidence(None, &pending, None),
            Err(EvidenceRejection::Missing)
        );
        let ev = evidence("user-other", true, 2_000);
        assert_eq!(
            evaluate_evidence(Some(&ev), &pending, None),
            Err(EvidenceRejection::MismatchedUser)
        );
        let ev = evidence("user-aman", false, 2_000);
        assert_eq!(
            evaluate_evidence(Some(&ev), &pending, None),
            Err(EvidenceRejection::NotFromUser)
        );
        let ev = evidence("user-aman", true, 999);
        assert_eq!(
            evaluate_evidence(Some(&ev), &pending, None),
            Err(EvidenceRejection::Stale)
        );
        let ev = evidence("user-aman", true, 2_000);
        assert_eq!(
            evaluate_evidence(Some(&ev), &pending, Some("agent-a")),
            Err(EvidenceRejection::SelfMediation)
        );
    }

    #[test]
    fn rejection_descriptions_are_non_empty() {
        for r in [
            EvidenceRejection::Missing,
            EvidenceRejection::MismatchedUser,
            EvidenceRejection::NotFromUser,
            EvidenceRejection::Stale,
            EvidenceRejection::SelfMediation,
        ] {
            assert!(!r.describe().is_empty());
        }
    }
}
