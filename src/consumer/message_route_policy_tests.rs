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

/// Or 组声明顺序即优先级：哨兵是保留字，恒声明在最前——与领域规则
/// （知会 / 用户来源）同时成立时哨兵胜出（均为 Discard，reason 为哨兵文案）。
/// 修复前 user_origin 会短路，把 "NO_REPLY" 原文发给用户（2026-10-16 实证）
#[test]
fn static_route_sentinel_wins_over_domain_rules() {
    // Agent 来源 + 知会 + 哨兵同时成立
    assert_eq!(
        judge_static_reply_route(input(
            MessageRole::Agent,
            "agent-2",
            MessageType::AgentNotify,
            NO_REPLY_SENTINEL
        )),
        Some(AutoReplyRoute::Discard {
            reason: "agent final output is the NO_REPLY sentinel"
        })
    );
}

/// 回归（2026-10-16）：用户身份中继的任务派发 from_role=User，Agent Final
/// 输出 NO_REPLY 时若 user_origin 先命中，会把哨兵原文当回复发给用户
/// （全库实证 29 条泄漏、25+ 条直达用户）。哨兵优先级最高，任何来源
/// 全等即 Discard
#[test]
fn static_route_discards_no_reply_sentinel_for_user_origin() {
    assert_eq!(
        judge_static_reply_route(input(
            MessageRole::User,
            "user-1",
            MessageType::Text,
            NO_REPLY_SENTINEL
        )),
        Some(AutoReplyRoute::Discard {
            reason: "agent final output is the NO_REPLY sentinel"
        })
    );
}

/// 协作压力硬红线：纯文本往返撞窗口上限 → 知会收口（CloseChainAsNotify），
/// 既完成最后一轮信息传递、又必然终止链条；
/// 关键回归：有新信息回血时窗口重置，硬窗口之外仍允许 Peer 继续协作
#[test]
fn chain_route_escalates_at_pressure_wall_but_recovery_keeps_peer() {
    use crate::pkg::collaboration_pressure::{ChainHop, ChainPressure, Novelty};

    fn chain(rounds: usize, novelty_at: &[usize]) -> ChainPressure {
        let mut pressure = ChainPressure::default();
        for i in 0..rounds {
            let novelty = if novelty_at.contains(&i) {
                Novelty::ToolResult
            } else {
                Novelty::None
            };
            pressure.observe(ChainHop {
                from_agent: true,
                novelty,
            });
        }
        pressure
    }

    assert_eq!(
        judge_chain_reply_route(&chain(0, &[])),
        AutoReplyRoute::Peer
    );
    assert_eq!(
        judge_chain_reply_route(&chain(1, &[])),
        AutoReplyRoute::Peer
    );
    assert_eq!(
        judge_chain_reply_route(&chain(7, &[])),
        AutoReplyRoute::Peer
    );
    assert!(matches!(
        judge_chain_reply_route(&chain(8, &[])),
        AutoReplyRoute::CloseChainAsNotify { .. }
    ));
    assert_eq!(
        judge_chain_reply_route(&chain(12, &[3, 7, 11])),
        AutoReplyRoute::Peer
    );
}

/// 回归（2026-10-18）：模型把哨兵写成中文「未回复」——全库 grep / git 历史 /
/// seeds / soul 均无该字面量，只能来自模型输出。英文全等不命中，该 Final 被
/// 当成真实回复投给财务总监，引发两轮无意义往返（一次确认 + 一次澄清）。
/// 词表内的等价写法必须与主哨兵等效：任何来源一律 Discard
#[test]
fn static_route_discards_no_reply_sentinel_variants() {
    for variant in NO_REPLY_SENTINEL_VARIANTS {
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::Agent,
                "agent-2",
                MessageType::Text,
                variant
            )),
            Some(AutoReplyRoute::Discard {
                reason: SENTINEL_HIT_REASON
            }),
            "哨兵写法 {variant:?} 必须命中"
        );
        // 用户身份中继的任务派发同样受词表约束（from_role=User 不得短路）
        assert_eq!(
            judge_static_reply_route(input(
                MessageRole::User,
                "user-1",
                MessageType::Text,
                variant
            )),
            Some(AutoReplyRoute::Discard {
                reason: SENTINEL_HIT_REASON
            }),
            "哨兵写法 {variant:?} 在用户来源下也必须命中"
        );
    }
}

/// 归一化容忍：大小写差异与收尾标点/引号不应让模型躲过哨兵识别
#[test]
fn static_route_discards_normalized_sentinel_forms() {
    for raw in [
        "no_reply",
        "No_Reply",
        "NO_REPLY.",
        "  NO_REPLY  ",
        "\"NO_REPLY\"",
        "未回复。",
    ] {
        assert_eq!(
            judge_static_reply_route(input(MessageRole::Agent, "agent-2", MessageType::Text, raw)),
            Some(AutoReplyRoute::Discard {
                reason: SENTINEL_HIT_REASON
            }),
            "{raw:?} 归一化后应命中哨兵"
        );
    }
}

/// 包含但不全等 → 不命中：正文中提到「无需回复」是合法内容，
/// 只有独占全文才算哨兵（判定恒为全等，绝不做包含匹配）
#[test]
fn static_route_ignores_sentinel_inside_prose() {
    for raw in [
        "这笔账无需回复",
        "我判断该请求无需跟进，但仍需你确认金额",
        "NO_REPLY 是哨兵",
        "如果你不需要回复就不用管",
    ] {
        assert_eq!(
            judge_static_reply_route(input(MessageRole::Agent, "agent-2", MessageType::Text, raw)),
            None,
            "{raw:?} 是正常正文，不得被哨兵误伤"
        );
    }
}

/// 回复义务只看入口消息语义：知会默认不回，对等协作在等答复
///
/// 关键边界：用户/系统来源与自触发**不进入**仲裁（投递方向由来源规则决定），
/// 这样主流用户对话的 Prompt 与路由都零变化。
#[test]
fn reply_obligation_follows_message_semantics() {
    assert_eq!(
        ReplyObligation::of(MessageRole::Agent, "agent-2", "agent-1", MessageType::Text),
        Some(ReplyObligation::Expected)
    );
    assert_eq!(
        ReplyObligation::of(
            MessageRole::Agent,
            "agent-2",
            "agent-1",
            MessageType::AgentNotify
        ),
        Some(ReplyObligation::NotifyOnly)
    );
    assert_eq!(
        ReplyObligation::of(MessageRole::User, "user-1", "agent-1", MessageType::Text),
        None
    );
    assert_eq!(
        ReplyObligation::of(
            MessageRole::System,
            "task-1",
            "agent-1",
            MessageType::TaskAssignment
        ),
        None
    );
    // 自触发：from == 本 Agent，不构成对等对话
    assert_eq!(
        ReplyObligation::of(MessageRole::Agent, "agent-1", "agent-1", MessageType::Text),
        None
    );
}

/// 提示词必须把两种合法形态与判据讲清楚，且明确「由你决定」
///
/// 同时钉住「提示词承诺 = 路由行为」：文案说 NO_REPLY 不回发，路由就必须
/// Discard；两边漂移会让模型学会一个不存在的出口。
#[test]
fn reply_protocol_prompt_states_both_forms() {
    let prompt = ReplyObligation::Expected.prompt_lines().join("\n");
    assert!(prompt.contains("是否回复由你决定"), "{prompt}");
    assert!(prompt.contains("正文即答复全文"), "{prompt}");
    assert!(prompt.contains("NO_REPLY"), "{prompt}");
    assert!(prompt.contains("独占全文"), "{prompt}");
    assert_eq!(
        judge_static_reply_route(input(
            MessageRole::Agent,
            "agent-2",
            MessageType::Text,
            "NO_REPLY"
        )),
        Some(AutoReplyRoute::Discard {
            reason: "agent final output is the NO_REPLY sentinel"
        })
    );
}

/// 知会消息：框架不回发 Final，提示词必须给出「确需回应走工具」的出口
///
/// 否则 Agent 想回也没有合法通道（Final 被 Discard），只能沉默或反复输出正文。
#[test]
fn notify_obligation_prompt_points_to_explicit_send() {
    let prompt = ReplyObligation::NotifyOnly.prompt_lines().join("\n");
    assert!(prompt.contains("知会消息"), "{prompt}");
    assert!(prompt.contains("不会回发"), "{prompt}");
    assert!(prompt.contains("发送消息工具"), "{prompt}");
    assert_eq!(
        judge_static_reply_route(input(
            MessageRole::Agent,
            "agent-2",
            MessageType::AgentNotify,
            "进度同步：已完成 60%"
        )),
        Some(AutoReplyRoute::Discard {
            reason: "agent-notify message (sender declared no reply needed)"
        })
    );
}

/// Final 决策解析：只有哨兵判 NoReply，带解释的写法一律按正文处理
///
/// 宁可多发一次（对方再判一次无需回复），也绝不把「NO_REPLY: 这条不用回」
/// 这种自造格式当哨兵吞掉——它不是封闭词表成员，也不该被 substring 兜底。
#[test]
fn reply_decision_only_fails_closed_on_sentinel() {
    for raw in ["NO_REPLY", " no_reply. ", "无需回复"] {
        assert_eq!(
            classify_reply_decision(raw),
            ReplyDecision::NoReply,
            "{raw:?}"
        );
    }
    for raw in ["改好了", "NO_REPLY: 这条不用回", "NO_REPLY 是哨兵", ""] {
        assert_eq!(
            classify_reply_decision(raw),
            ReplyDecision::Reply,
            "{raw:?}"
        );
    }
}

/// 词表护栏：封闭集合必须含英文主哨兵，且无空白/重复/空条目
/// （有人偷偷把词表清空 => 2026-10-18 泄漏现场回归，测试必须红）
#[test]
fn no_reply_sentinel_variants_stays_closed_and_clean() {
    assert!(
        NO_REPLY_SENTINEL_VARIANTS.contains(&NO_REPLY_SENTINEL),
        "词表必须包含英文主哨兵"
    );
    for variant in NO_REPLY_SENTINEL_VARIANTS {
        assert!(!variant.is_empty(), "词表不得含空条目");
        assert_eq!(
            variant.trim(),
            *variant,
            "词表条目 {variant:?} 不得含首尾空白"
        );
        assert_eq!(
            NO_REPLY_SENTINEL_VARIANTS
                .iter()
                .filter(|v| *v == variant)
                .count(),
            1,
            "词表条目 {variant:?} 重复"
        );
    }
}
