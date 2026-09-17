//! Final 自动回发路由策略（策略引擎的消息路由领域落地）
//!
//! 复用 `pkg/policy` 引擎组合路由策略组（照 shell_policy 先例）：
//! - 静态判定段（零查库）：领域匹配器规则（来源角色 / 自触发 / 知会类型）+
//!   builtin 通用 `FieldEqualsPolicy`（NO_REPLY 哨兵全等）按 Or 组合，
//!   声明顺序即判定优先级（首个命中即产出路由）
//! - 机械兜底段：builtin 通用 `ThresholdPolicy`（`chain_depth >= 上限` →
//!   升级归属用户）。仅对「跨 Agent 对等回复候选」执行——需要沿 reply_to_id
//!   查库计算链深度（见 `MessageConsumer::agent_reply_chain_depth`），
//!   静态命中的场景零额外查询
//!
//! 领域特有判定保留专用匹配器（不具备跨领域共性，抽象边界参照 builtin 模块文档）：
//! - 来源角色 / 知会类型是枚举等值（Metrics 侧以数值承载）
//! - 自触发是双键等值（`from_id == agent_id`），通用 FieldEqualsPolicy 只支持
//!   「单键 == 常量」，跨键比较属路由特有形态

use std::sync::OnceLock;

use common::enums::{MessageRole, MessageType};

use crate::pkg::policy::builtin::{FieldEqualsPolicy, ThresholdPolicy};
use crate::pkg::policy::{Metrics, Policy, PolicyBuilder};

/// Metrics 键约定（入口统一填充）
pub mod keys {
    /// 消息来源角色（`MessageRole::to_i32` 数值）
    pub const FROM_ROLE: &str = "route.from_role";
    /// 来源 id（用户 id / Agent id / 触发器标识）
    pub const FROM_ID: &str = "route.from_id";
    /// 本次被唤醒的 Agent id（自触发判定的对侧键）
    pub const AGENT_ID: &str = "route.agent_id";
    /// 消息类型（`MessageType::to_i32` 数值）
    pub const MESSAGE_TYPE: &str = "route.message_type";
    /// 唤醒 Final 原始输出（trim 后）
    pub const RAW_OUTPUT: &str = "route.raw_output";
    /// 回复链深度（机械兜底段专用，由 consumer 查库计算后填充）
    pub const CHAIN_DEPTH: &str = "route.chain_depth";
}

/// Agent 间回复链深度上限：入口消息沿 `reply_to_id` 链上连续 Agent 来源消息
/// 达到该条数时不再自动回发，改为通知归属用户（防 A↔B 无限乒乓的最后防线）
pub const MAX_AGENT_REPLY_CHAIN: usize = 5;

/// 无需回复哨兵：Agent 判断「后续工作与来源方无关」时，让 Final 恰好输出该词，
/// Framework 检测到后不再把 Final 回发给来源 Agent。仅做 trim 后全等匹配，
/// 防止误伤恰好包含该词的正常正文
pub const NO_REPLY_SENTINEL: &str = "NO_REPLY";

/// Final 自动回发的路由决策
///
/// `handle_agent_message` 唤醒完成后按此决定 Final 输出的去向：
/// - `Peer`            → 回发给来源方（用户或对端 Agent）
/// - `Discard`         → 丢弃 Final（无对等回复对象 / 发送方声明无需回复）
/// - `EscalateToOwner` → 终止回复链并通知归属用户（防乒乓机械兜底）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoReplyRoute {
    Peer,
    Discard { reason: &'static str },
    EscalateToOwner { reason: &'static str },
}

/// 静态路由判定的输入（consumer 从 Message / 唤醒结果解析后填充）
pub struct RouteInput<'a> {
    /// 消息来源角色
    pub from_role: MessageRole,
    /// 来源 id（用户 id / Agent id / 触发器标识）
    pub from_id: &'a str,
    /// 本次被唤醒的 Agent id
    pub agent_id: &'a str,
    /// 消息类型
    pub message_type: MessageType,
    /// 唤醒 Final 原始输出（trim 后）
    pub raw_output: &'a str,
}

/// 路由匹配器（路由领域特有的判定形态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteMatcher {
    /// 来源角色等值（用户 / 系统来源的粗分类）
    RoleEquals(MessageRole),
    /// 双键等值：`from_id == agent_id`（自触发守卫）
    SelfTrigger,
    /// 消息类型等值（知会声明）
    MessageTypeEquals(MessageType),
}

impl RouteMatcher {
    fn matches(&self, metrics: &Metrics) -> bool {
        match self {
            RouteMatcher::RoleEquals(role) => {
                metrics.get_u64(keys::FROM_ROLE) == Some(role.to_i32() as u64)
            }
            RouteMatcher::SelfTrigger => {
                metrics.get_str(keys::FROM_ID) == metrics.get_str(keys::AGENT_ID)
            }
            RouteMatcher::MessageTypeEquals(message_type) => {
                metrics.get_u64(keys::MESSAGE_TYPE) == Some(message_type.to_i32() as u64)
            }
        }
    }
}

/// 静态路由规则（通用 `Policy` 的消息路由领域实现）
#[derive(Debug, Clone, Copy)]
struct StaticRoutePolicy {
    id: &'static str,
    name: &'static str,
    condition_desc: &'static str,
    matcher: RouteMatcher,
    /// 命中产出：路由决策由规则表声明式携带（不走引擎 PolicyAction——
    /// Peer/Discard/Escalate 与 Deny/Confirm/Audit 是两套语义）
    outcome: AutoReplyRoute,
}

impl Policy for StaticRoutePolicy {
    fn id(&self) -> &str {
        self.id
    }

    fn name(&self) -> &str {
        self.name
    }

    fn condition_desc(&self) -> &str {
        self.condition_desc
    }

    fn required_metrics(&self) -> Vec<String> {
        match self.matcher {
            RouteMatcher::RoleEquals(_) => vec![keys::FROM_ROLE.to_string()],
            RouteMatcher::SelfTrigger => {
                vec![keys::FROM_ID.to_string(), keys::AGENT_ID.to_string()]
            }
            RouteMatcher::MessageTypeEquals(_) => vec![keys::MESSAGE_TYPE.to_string()],
        }
    }

    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        if self.matcher.matches(metrics) {
            vec![self.id.to_string()]
        } else {
            Vec::new()
        }
    }
}

/// 静态规则表（声明顺序 = 判定优先级：首个命中即产出路由）
static STATIC_ROUTE_DEFS: &[StaticRoutePolicy] = &[
    // 用户来源 → Peer：用户身份中继后 Final 自然回到该用户
    StaticRoutePolicy {
        id: "user_origin",
        name: "用户来源",
        condition_desc: "from_role == User",
        matcher: RouteMatcher::RoleEquals(MessageRole::User),
        outcome: AutoReplyRoute::Peer,
    },
    // 系统来源 → Discard：触发器消息（纯系统指令、无归属用户的 A2A 触达）没有对等回复对象
    StaticRoutePolicy {
        id: "system_origin",
        name: "系统来源",
        condition_desc: "from_role == System",
        matcher: RouteMatcher::RoleEquals(MessageRole::System),
        outcome: AutoReplyRoute::Discard {
            reason: "system-originated message has no peer reply target",
        },
    },
    // 自触发 → Discard：触发器以 Owner Agent 名义发给自己的消息，回给自己会无限自唤醒循环
    StaticRoutePolicy {
        id: "self_trigger",
        name: "自触发守卫",
        condition_desc: "from_id == agent_id",
        matcher: RouteMatcher::SelfTrigger,
        outcome: AutoReplyRoute::Discard {
            reason: "agent self-triggered message (from == to)",
        },
    },
    // 知会 → Discard：发送方已声明无需回复。仅 Agent 来源会命中该规则——
    // 用户来源已被声明在前的 user_origin 拦下，用户对话主干不受影响
    StaticRoutePolicy {
        id: "agent_notify",
        name: "知会消息",
        condition_desc: "message_type == AgentNotify",
        matcher: RouteMatcher::MessageTypeEquals(MessageType::AgentNotify),
        outcome: AutoReplyRoute::Discard {
            reason: "agent-notify message (sender declared no reply needed)",
        },
    },
];

/// 通用哨兵策略 id（FieldEqualsPolicy 成员，产出见 `static_outcome` 的表外分支）
const SENTINEL_POLICY_ID: &str = "no_reply_sentinel";
/// 哨兵命中的路由产出 reason
const SENTINEL_HIT_REASON: &str = "agent final output is the NO_REPLY sentinel";

/// 静态规则集：领域规则表 + 通用哨兵策略按 Or 组合
/// （等价 policy_set!(OR { .. }) 的展开结果，声明顺序即优先级）
fn static_ruleset() -> &'static dyn Policy {
    static RULESET: OnceLock<Box<dyn Policy>> = OnceLock::new();
    RULESET
        .get_or_init(|| {
            let mut builder = PolicyBuilder::new();
            for rule in STATIC_ROUTE_DEFS {
                builder = builder.with_policy(*rule);
            }
            builder
                .with_policy(FieldEqualsPolicy::new(
                    SENTINEL_POLICY_ID,
                    "NoReplySentinel",
                    keys::RAW_OUTPUT,
                    NO_REPLY_SENTINEL,
                    "Final 全等 NO_REPLY",
                ))
                .or()
        })
        .as_ref()
}

/// 链深度兜底策略：通用 ThresholdPolicy 承载
/// （`chain_depth >= MAX_AGENT_REPLY_CHAIN` 即命中；上限为正常量，恒启用）
fn chain_policy() -> &'static ThresholdPolicy {
    static POLICY: OnceLock<ThresholdPolicy> = OnceLock::new();
    POLICY.get_or_init(|| {
        ThresholdPolicy::new(
            "chain_depth_escalation",
            "ChainDepthEscalation",
            keys::CHAIN_DEPTH,
            MAX_AGENT_REPLY_CHAIN as u64,
            "回复链深度",
            "",
        )
    })
}

/// 静态命中 id → 路由产出：领域规则产出内嵌在规则表；表外唯一成员是
/// 通用 FieldEqualsPolicy 哨兵策略，命中即 Discard
fn static_outcome(policy_id: &str) -> AutoReplyRoute {
    STATIC_ROUTE_DEFS
        .iter()
        .find(|rule| rule.id == policy_id)
        .map(|rule| rule.outcome)
        .unwrap_or(AutoReplyRoute::Discard {
            reason: SENTINEL_HIT_REASON,
        })
}

/// Final 回发路由的静态判定段（无需查库）
///
/// **单一扩展点**：所有「没有对等回复对象 / 回复会造成自唤醒 / 发送方声明无需
/// 回复」的场景都在静态规则表里声明，新增场景只加一条规则。返回 `None` 表示
/// 「跨 Agent 对等回复候选」，需继续做回复链深度机械判定
/// （`judge_chain_reply_route`）。
///
/// | 场景 | 规则 | 路由 | 理由 |
/// |------|------|------|------|
/// | 用户消息 | `user_origin` | Peer | 用户身份中继后 Final 自然回到该用户 |
/// | System 来源 | `system_origin` | Discard | 触发器消息（纯系统指令、无归属用户的 A2A 触达）没有对等回复对象 |
/// | Agent 自触发 | `self_trigger` | Discard | 触发器以 Owner Agent 名义发给自己的消息，回给自己会无限自唤醒循环 |
/// | 知会消息 | `agent_notify` | Discard | 发送方已声明无需回复；仅 Agent 来源生效，用户对话主干不受影响 |
/// | 无需回复哨兵 | `no_reply_sentinel`（通用 FieldEqualsPolicy） | Discard | Agent 自判后续工作与来源方无关，回发只会制造乒乓 |
pub fn judge_static_reply_route(input: RouteInput<'_>) -> Option<AutoReplyRoute> {
    let metrics = Metrics::new()
        .with(keys::FROM_ROLE, input.from_role.to_i32() as u64)
        .with(keys::FROM_ID, input.from_id)
        .with(keys::AGENT_ID, input.agent_id)
        .with(keys::MESSAGE_TYPE, input.message_type.to_i32() as u64)
        .with(keys::RAW_OUTPUT, input.raw_output);

    // Or 组：按声明顺序上浮首个命中（领域规则在前，哨兵兜底在后）
    static_ruleset()
        .evaluate(&metrics)
        .first()
        .map(|id| static_outcome(id))
}

/// Final 回发路由的链深度机械判定段（防乒乓最后防线）
///
/// 链深度 = 入口消息沿 `reply_to_id` 向上连续 Agent 来源消息的条数（含入口自身），
/// 由 consumer 查库计算（见 `MessageConsumer::agent_reply_chain_depth`）。
/// A↔B 协作中每条自动回复的 reply_to_id 都指向触发它的消息，链深度随往返单调
/// 递增；达到 `MAX_AGENT_REPLY_CHAIN` 说明模型侧的协调约定（哨兵 / 知会声明）
/// 已失效，不再信任模型判断，机械终止回发并升级给归属用户。
pub fn judge_chain_reply_route(chain_depth: usize) -> AutoReplyRoute {
    let metrics = Metrics::new().with(keys::CHAIN_DEPTH, chain_depth as u64);
    if chain_policy().is_triggered(&metrics) {
        AutoReplyRoute::EscalateToOwner {
            reason: "agent reply chain depth limit exceeded",
        }
    } else {
        AutoReplyRoute::Peer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        from_role: MessageRole,
        from_id: &'a str,
        message_type: MessageType,
        raw_output: &'a str,
    ) -> RouteInput<'a> {
        RouteInput {
            from_role,
            from_id,
            agent_id: "agent-1",
            message_type,
            raw_output,
        }
    }

    /// System 来源消息恒走 Discard（无对等回复对象）
    #[test]
    fn static_route_discards_system_origin() {
        for from_id in ["task-1", "scheduler"] {
            assert_eq!(
                judge_static_reply_route(input(
                    MessageRole::System,
                    from_id,
                    MessageType::Text,
                    "done"
                )),
                Some(AutoReplyRoute::Discard {
                    reason: "system-originated message has no peer reply target"
                })
            );
        }
    }

    /// Agent 自触发守卫：from == to 回给自己会无限自唤醒，必须 Discard；
    /// 跨 Agent 协作（from != to）返回 None，交由链深度兜底继续判定
    #[test]
    fn static_route_discards_agent_self_trigger() {
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::Agent,
                "agent-1",
                MessageType::Text,
                "done"
            )),
            Some(AutoReplyRoute::Discard {
                reason: "agent self-triggered message (from == to)"
            })
        );
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::Agent,
                "agent-2",
                MessageType::Text,
                "done"
            )),
            None
        );
    }

    /// 用户消息恒走对等回复（用户身份中继后 Final 自然回到该用户）
    #[test]
    fn static_route_peers_user_origin() {
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::User,
                "user-1",
                MessageType::Text,
                "done"
            )),
            Some(AutoReplyRoute::Peer)
        );
    }

    /// 知会消息守卫：发送方声明无需回复 → Discard（仅 Agent 来源生效，
    /// User 来源的 AgentNotify 被 user_origin 先行拦下，不影响用户对话主干）
    #[test]
    fn static_route_discards_agent_notify() {
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::Agent,
                "agent-2",
                MessageType::AgentNotify,
                "noted"
            )),
            Some(AutoReplyRoute::Discard {
                reason: "agent-notify message (sender declared no reply needed)"
            })
        );
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::User,
                "user-1",
                MessageType::AgentNotify,
                "noted"
            )),
            Some(AutoReplyRoute::Peer)
        );
    }

    /// NO_REPLY 哨兵：Final 全等命中 → Discard；包含但不全等不误伤
    #[test]
    fn static_route_discards_no_reply_sentinel() {
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::Agent,
                "agent-2",
                MessageType::Text,
                NO_REPLY_SENTINEL
            )),
            Some(AutoReplyRoute::Discard {
                reason: "agent final output is the NO_REPLY sentinel"
            })
        );
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::Agent,
                "agent-2",
                MessageType::Text,
                "I will NO_REPLY if needed"
            )),
            None
        );
    }

    /// Or 组声明顺序即优先级：Agent 来源 + 知会 + 哨兵同时成立时，
    /// 知会规则（声明在前）胜出，reason 为知会文案
    #[test]
    fn static_route_order_notify_wins_over_sentinel() {
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::Agent,
                "agent-2",
                MessageType::AgentNotify,
                NO_REPLY_SENTINEL
            )),
            Some(AutoReplyRoute::Discard {
                reason: "agent-notify message (sender declared no reply needed)"
            })
        );
    }

    /// 链深度机械兜底：达到上限 → EscalateToOwner，未达 → Peer
    #[test]
    fn chain_route_escalates_at_depth_limit() {
        assert_eq!(judge_chain_reply_route(0), AutoReplyRoute::Peer);
        assert_eq!(judge_chain_reply_route(1), AutoReplyRoute::Peer);
        assert_eq!(
            judge_chain_reply_route(MAX_AGENT_REPLY_CHAIN - 1),
            AutoReplyRoute::Peer
        );
        assert!(matches!(
            judge_chain_reply_route(MAX_AGENT_REPLY_CHAIN),
            AutoReplyRoute::EscalateToOwner { .. }
        ));
        assert!(matches!(
            judge_chain_reply_route(MAX_AGENT_REPLY_CHAIN + 1),
            AutoReplyRoute::EscalateToOwner { .. }
        ));
    }
}
