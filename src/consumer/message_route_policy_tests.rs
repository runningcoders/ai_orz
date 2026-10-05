//! tests 单元测试（拆分自 message_route_policy.rs）
//!
//! 文件瘦身：原 456 行 → 296 行，测试体 161 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

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
