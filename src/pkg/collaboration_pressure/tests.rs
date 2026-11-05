//! 协作压力组件护栏测试
//!
//! 重点钉住三件容易 regressions 的事：
//! 1. 回血必须**重置连续窗口**（否则新鲜感延长不了协作）；
//! 2. 阈值从 Metrics 注入，不固化进规则对象（否则配置改了不生效）；
//! 3. 每个档位的行为动作与配套提示词来自**同一行规则**（防止改动作忘改提示词）。

use super::policy::STAGE_DEFS;
use super::*;

fn plain() -> ChainHop {
    ChainHop::plain()
}

fn hop(novelty: Novelty) -> ChainHop {
    ChainHop {
        from_agent: true,
        novelty,
    }
}

#[test]
fn fatigue_accumulates_per_hop_and_enters_pressure_band() {
    let mut pressure = ChainPressure::default();
    for _ in 0..3 {
        assert!(pressure.observe(plain()));
    }
    assert_eq!(pressure.depth(), 3);
    assert_eq!(pressure.window_depth(), 3);
    assert_eq!(pressure.fatigue(), 36);

    let decision = pressure.judge();
    assert_eq!(decision.stage, PressureStage::Pressured);
    assert_eq!(decision.action, PressureAction::Nudge);
    assert_eq!(decision.policy_id, "pressure_nudge");
}

#[test]
fn pure_pingpong_hits_hard_wall_at_window_limit() {
    let mut pressure = ChainPressure::default();
    for _ in 0..8 {
        pressure.observe(plain());
    }
    assert_eq!(pressure.window_depth(), 8);
    assert_eq!(pressure.judge().action, PressureAction::Escalate);
    assert_eq!(pressure.judge().policy_id, "pressure_window_exhausted");
}

/// 核心护栏：回血让协作合法超过硬窗口，且绝对链深仍单调（只用于展示）
#[test]
fn fresh_input_lets_chain_outlive_hard_window() {
    let mut plain_chain = ChainPressure::default();
    let mut fresh_chain = ChainPressure::default();
    for i in 0..12 {
        plain_chain.observe(plain());
        let novelty = if i % 4 == 3 {
            Novelty::ToolResult
        } else {
            Novelty::None
        };
        fresh_chain.observe(hop(novelty));
    }

    assert_eq!(plain_chain.judge().action, PressureAction::Escalate);
    assert_eq!(fresh_chain.judge().action, PressureAction::Continue);
    assert_eq!(fresh_chain.judge().stage, PressureStage::Recovered);
    assert!(fresh_chain.depth() > fresh_chain.config().hard_window_depth as usize);
    assert_eq!(fresh_chain.recovery_count(), 3);
}

#[test]
fn novelty_recovers_fatigue_and_resets_only_the_window() {
    let mut pressure = ChainPressure::default();
    for _ in 0..3 {
        pressure.observe(plain());
    }
    assert_eq!(pressure.fatigue(), 36);

    assert!(pressure.observe(hop(Novelty::ToolResult)));
    assert_eq!(pressure.fatigue(), 8, "36 + 12 - 40");
    assert_eq!(pressure.window_depth(), 0);
    assert_eq!(pressure.depth(), 4, "绝对链深不因回血回退");
    assert_eq!(pressure.recovery_count(), 1);
    assert_eq!(pressure.judge().stage, PressureStage::Recovered);
}

#[test]
fn distinct_content_only_trims_fatigue_without_resetting_window() {
    let mut pressure = ChainPressure::default();
    for _ in 0..3 {
        pressure.observe(plain());
    }
    pressure.observe(hop(Novelty::DistinctContent));
    assert_eq!(pressure.fatigue(), 40, "36 + 12 - 8");
    assert_eq!(pressure.window_depth(), 4, "弱新信息不应重置窗口");
    assert_eq!(pressure.recovery_count(), 0);
}

#[test]
fn external_input_resets_whole_chain_state() {
    let mut pressure = ChainPressure::default();
    for _ in 0..5 {
        pressure.observe(plain());
    }
    pressure.observe(hop(Novelty::ExternalInput));
    assert_eq!(pressure.depth(), 1);
    assert_eq!(pressure.window_depth(), 0);
    assert_eq!(pressure.fatigue(), 0);
    assert_eq!(pressure.reset_count(), 1);
    assert_eq!(pressure.judge().stage, PressureStage::Fresh);
}

#[test]
fn non_agent_hop_breaks_chain() {
    let mut pressure = ChainPressure::default();
    assert!(pressure.observe(plain()));
    assert!(!pressure.observe(ChainHop {
        from_agent: false,
        novelty: Novelty::None,
    }));
    assert_eq!(pressure.depth(), 1, "断点之后不应继续计数");
}

/// 阈值来自 Metrics：同一组无状态规则，换配置立刻换档位
#[test]
fn thresholds_are_not_baked_into_the_ruleset() {
    let default_pressure = {
        let mut p = ChainPressure::default();
        p.observe(plain());
        p
    };
    let custom = PressureConfig {
        soft_fatigue: 10,
        hard_fatigue: 50,
        hard_window_depth: 3,
        ..PressureConfig::default()
    };
    let mut tight = ChainPressure::new(custom);
    tight.observe(plain());

    assert_eq!(default_pressure.judge().stage, PressureStage::Fresh);
    assert_eq!(tight.judge().stage, PressureStage::Pressured);
    assert_eq!(tight.metrics().get_u64(keys::SOFT_FATIGUE), Some(10));
    assert_eq!(tight.metrics().get_u64(keys::HARD_FATIGUE), Some(50));
    assert_eq!(tight.metrics().get_u64(keys::HARD_WINDOW_DEPTH), Some(3));
}

#[test]
fn invalid_config_falls_back_to_default() {
    let broken = PressureConfig {
        soft_fatigue: 90,
        hard_fatigue: 10,
        ..PressureConfig::default()
    };
    assert_eq!(
        ChainPressure::new(broken).config(),
        PressureConfig::default()
    );
}

/// 动作 / 提示词 / 回血话术必须来自同一行规则
#[test]
fn every_hit_binds_action_and_prompt_to_the_same_rule() {
    let mut pressure = ChainPressure::default();
    let mut decisions = Vec::new();
    for i in 0..10 {
        let novelty = if i == 4 {
            Novelty::ToolResult
        } else {
            Novelty::None
        };
        pressure.observe(hop(novelty));
        decisions.push(pressure.judge());
    }

    let mut saw_table_hit = false;
    for decision in &decisions {
        if let Some(rule) = STAGE_DEFS.iter().find(|r| r.id == decision.policy_id) {
            saw_table_hit = true;
            assert_eq!(rule.stage, decision.stage);
            assert_eq!(rule.action, decision.action);
            assert_eq!(rule.prompt_hint, decision.prompt_hint);
            assert_eq!(rule.recovery_hint, decision.recovery_hint);
        }
        let lines = decision.prompt_lines();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("【协作压力】"));
        assert!(lines[1].contains(decision.action.label()));
        assert!(decision.log_line().contains("collab_pressure"));
    }
    assert!(saw_table_hit);
}

/// 硬墙不是「静默丢弃」：动作是收口为知会，配套文案必须说清「本轮仍会送达」
///
/// 这条护栏钉的是「改动作忘改文案」的反向版本：行为从「暂停回发」改成「知会
/// 收口」后，提示词若还写着「暂停/不发」，模型会以为结论发不出去，转而用工具
/// 重复投递或干脆不说清楚——两种都会把收口的价值抵消掉。
#[test]
fn exhausted_stage_prompt_matches_chain_close_behavior() {
    let walled: Vec<_> = STAGE_DEFS
        .iter()
        .filter(|rule| rule.action == PressureAction::Escalate)
        .collect();
    assert_eq!(walled.len(), 2, "两条硬墙规则都应标记为收口动作");
    for rule in walled {
        assert_eq!(rule.stage, PressureStage::Exhausted);
        assert!(
            rule.prompt_hint.contains("知会"),
            "硬墙文案必须说明 Final 会作为知会送达：{}",
            rule.prompt_hint
        );
        for forbidden in ["暂停", "不回发你的"] {
            assert!(
                !rule.prompt_hint.contains(forbidden),
                "硬墙文案残留旧口径「{forbidden}」：{}",
                rule.prompt_hint
            );
        }
        assert!(
            rule.recovery_hint.contains("主动") && rule.recovery_hint.contains("新消息"),
            "续接话术必须指向「主动发新消息开新链」：{}",
            rule.recovery_hint
        );
    }
}

/// 收口知会给接收方的提示词：疲劳度事实 + 「要推进就自己开新链」
#[test]
fn chain_closed_prompt_carries_fatigue_and_reopen_guidance() {
    let mut pressure = ChainPressure::default();
    for _ in 0..8 {
        pressure.observe(plain());
    }
    let decision = pressure.judge();
    assert!(decision.closes_chain_as_notify());

    let lines = decision.chain_closed_prompt_lines();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("【协作链收口】"));
    // 数值必须「实际值 + 硬阈值」分开表述，不能是 9/8 这种比例（见下条护栏）
    assert!(
        lines[0].contains(&format!(
            "已连续 {} 跳无新信息（硬阈值 {} 跳）",
            pressure.window_depth(),
            pressure.config().hard_window_depth
        )),
        "窗口事实必须带上：{}",
        lines[0]
    );
    assert!(
        lines[0].contains(&format!(
            "疲劳度 {}（硬阈值 {}）",
            pressure.fatigue(),
            pressure.config().hard_fatigue
        )),
        "疲劳度事实必须带上：{}",
        lines[0]
    );
    assert!(lines[1].contains("NO_REPLY"));
    assert!(lines[2].contains("send_message_to_agent"));
    assert!(lines[2].contains("新链"));

    // 未触顶的链不产生收口话术（普通知会不该被套上「链已终止」的口径）
    let mut light = ChainPressure::default();
    light.observe(plain());
    let light_decision = light.judge();
    assert!(!light_decision.closes_chain_as_notify());
}

/// 接收方视角必然比发送侧多一跳（知会自己也是一跳），数字会「超过」硬阈值
///
/// 若沿用 `当前/阈值` 比例写法，模型实际看到的是「已连续 9/8 跳」「疲劳度
/// 108/100」——数字比上限还大，读起来像统计出错，反而削弱这条信号的可信度。
/// 必须改成「实际值（硬阈值 X）」的并列表述。
#[test]
fn chain_closed_prompt_renders_overflow_as_value_plus_threshold() {
    let mut pressure = ChainPressure::default();
    for _ in 0..9 {
        pressure.observe(plain());
    }
    assert!(
        pressure.window_depth() as u64 > pressure.config().hard_window_depth,
        "本护栏的前提是「接收方窗口确实超过硬阈值」"
    );
    let lines = pressure.judge().chain_closed_prompt_lines();
    assert!(
        lines[0].contains(&format!(
            "已连续 {} 跳无新信息（硬阈值 {} 跳）",
            pressure.window_depth(),
            pressure.config().hard_window_depth
        )),
        "溢出场景也必须读成「值 + 阈值」：{}",
        lines[0]
    );
    assert!(
        !lines[0].contains(&format!(
            "{}/{}",
            pressure.window_depth(),
            pressure.config().hard_window_depth
        )),
        "禁止比例写法（会渲染成 9/8）：{}",
        lines[0]
    );
}

#[test]
fn snapshot_is_serializable_and_prints_core_signals() {
    let mut pressure = ChainPressure::default();
    for _ in 0..3 {
        pressure.observe(plain());
    }
    let decision = pressure.judge();
    let json = serde_json::to_string(&decision.snapshot).expect("snapshot 可序列化");
    assert!(json.contains("\"fatigue\":36"));
    assert!(json.contains("Pressured"));

    let line = decision.log_line();
    assert!(line.contains("depth=3"));
    assert!(line.contains("window=3/8"));
    assert!(line.contains("fatigue=36/100"));
    assert!(line.contains("action=nudge"));
    assert!(line.contains("pressure_nudge"));
}

/// 方向护栏：`reply_to_id` 回溯得到的是新→旧序列，必须归一化为时间正序
///
/// 回血清零 `window_depth` 是有方向的运算。倒序直接 observe 会把「回血之前
/// 的老跳」算成「最近回血之后的连续窗口」——本测试同时钉住两种后果：
/// 窗口误算，以及回血点在链头时硬墙被整段绕过（无限互发防不住）。
#[test]
fn backward_chain_observation_is_normalized_to_chronological_order() {
    // 时间正序：3 跳纯文本 → 工具结果回血 → 5 跳纯文本
    let mut chronological: Vec<ChainHop> = vec![plain(); 3];
    chronological.push(hop(Novelty::ToolResult));
    chronological.extend((0..5).map(|_| plain()));
    let backward: Vec<ChainHop> = chronological.iter().rev().copied().collect();

    let mut by_time = ChainPressure::default();
    assert_eq!(by_time.observe_chain(chronological), 9);
    assert_eq!(by_time.window_depth(), 5, "窗口只统计回血之后的跳");
    assert_eq!(by_time.recovery_count(), 1);
    assert_eq!(by_time.depth(), 9);

    // 旧实现把倒序序列直接 observe：窗口算成回血之前的 3 跳
    let mut wrong = ChainPressure::default();
    wrong.observe_chain(backward.clone());
    assert_eq!(wrong.window_depth(), 3, "倒序观测确实会得到错误的窗口");

    // 生产路径：回溯序列交给组件归一化，结果必须与正序完全一致
    let mut by_traversal = ChainPressure::default();
    assert_eq!(by_traversal.observe_backward_chain(backward), 9);
    assert_eq!(by_traversal.depth(), by_time.depth());
    assert_eq!(by_traversal.window_depth(), by_time.window_depth());
    assert_eq!(by_traversal.fatigue(), by_time.fatigue());
    assert_eq!(by_traversal.recovery_count(), by_time.recovery_count());
    assert_eq!(by_traversal.judge().policy_id, by_time.judge().policy_id);
}

/// 回血点在链头时，倒序观测会把其后的长串纯文本整段清零 ⇒ 硬墙永不触发
#[test]
fn normalization_keeps_the_hard_wall_after_an_early_recovery() {
    let mut chronological: Vec<ChainHop> = vec![hop(Novelty::ToolResult)];
    chronological.extend((0..10).map(|_| plain()));
    let backward: Vec<ChainHop> = chronological.iter().rev().copied().collect();

    let mut by_time = ChainPressure::default();
    by_time.observe_chain(chronological);
    assert_eq!(by_time.judge().action, PressureAction::Escalate);

    let mut wrong = ChainPressure::default();
    wrong.observe_chain(backward.clone());
    assert_ne!(
        wrong.judge().action,
        PressureAction::Escalate,
        "倒序观测会漏掉硬墙"
    );

    let mut normalized = ChainPressure::default();
    normalized.observe_backward_chain(backward);
    assert_eq!(
        normalized.judge().action,
        PressureAction::Escalate,
        "归一化后硬墙必须恢复"
    );
}

#[test]
fn hard_fatigue_above_default_max_is_not_silently_rejected() {
    let config = PressureConfig {
        hard_fatigue: 150,
        ..PressureConfig::default()
    };
    let pressure = ChainPressure::new(config);
    assert_eq!(pressure.config().hard_fatigue, 150);
    assert_eq!(pressure.config().max_fatigue, 150, "疲劳度上限跟随硬阈值");
    assert_eq!(pressure.config().soft_fatigue, 36, "其余配置不被连带回退");
    assert_eq!(pressure.config().hard_window_depth, 8);
}
