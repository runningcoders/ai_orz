use super::*;
use crate::service::domain::runtime::compaction::estimate_tokens;

/// 私聊不参与群规则：即使前缀 / 指向都不满足也放行
#[test]
fn private_chat_always_passes() {
    assert_eq!(
        group_trigger_gate(false, false, "prefix", "bot", "hi"),
        InboundGateDecision::Process
    );
    assert_eq!(
        group_trigger_gate(false, false, "mention", "", ""),
        InboundGateDecision::Process
    );
}

/// mention 模式：明确指向 Agent 放行，否则丢弃
#[test]
fn mention_mode_requires_addressing_agent() {
    assert_eq!(
        group_trigger_gate(true, true, "mention", "", "帮我看看"),
        InboundGateDecision::Process
    );
    assert_eq!(
        group_trigger_gate(true, false, "mention", "", "闲聊"),
        InboundGateDecision::Skip
    );
    // 未知策略按 mention 处理
    assert_eq!(
        group_trigger_gate(true, false, "unexpected", "", "闲聊"),
        InboundGateDecision::Skip
    );
}

/// prefix 模式：正文去前导空白后以前缀开头才放行
#[test]
fn prefix_mode_matches_after_trimming() {
    assert_eq!(
        group_trigger_gate(true, false, "prefix", "bot", "  bot 帮我查一下"),
        InboundGateDecision::Process
    );
    assert_eq!(
        group_trigger_gate(true, false, "prefix", "bot", "普通聊天"),
        InboundGateDecision::Skip
    );
}

/// 前缀为空时降级为 mention，绝不能当成「匹配一切」
#[test]
fn empty_prefix_falls_back_to_mention() {
    assert_eq!(
        group_trigger_gate(true, true, "prefix", "  ", "你好"),
        InboundGateDecision::Process
    );
    assert_eq!(
        group_trigger_gate(true, false, "prefix", "", "你好"),
        InboundGateDecision::Skip
    );
}

/// all 模式：规则层全部放行，交由小脑判断
#[test]
fn all_mode_defers_to_cerebellum() {
    assert_eq!(
        group_trigger_gate(true, false, "all", "", "闲聊"),
        InboundGateDecision::Judge
    );
}

/// 小脑无结论（降级）必须放行；达到阈值才回复
#[test]
fn relevance_degrades_to_pass() {
    assert!(passes_relevance(None));
    assert!(passes_relevance(Some(RELEVANCE_THRESHOLD)));
    assert!(passes_relevance(Some(0.9)));
    assert!(!passes_relevance(Some(RELEVANCE_THRESHOLD - 0.01)));
}

/// 上下文按「身份 → 群聊 → 相关记忆 → 最近记忆 → 本次消息」拼接，空段落省略
#[test]
fn context_assembles_sections_in_order() {
    // 行文本由 Message::to_context_line() 预先格式化
    let history = vec!["用户: 今天发版吗".to_string(), "Agent: 周五发".to_string()];
    let context = build_group_context(
        None,
        &["长期偏好：周五发版".to_string()],
        &["上次讨论过发版窗口".to_string()],
        &history,
        "  那这次提前到周四？  ",
        "ou_sender",
        None,
    );
    let expected = "【最近群聊】\n用户: 今天发版吗\nAgent: 周五发\n\n\
【相关记忆】\n- 上次讨论过发版窗口\n\n\
【最近记忆】\n- 长期偏好：周五发版\n\n\
【本次消息】\n发送者: ou_sender\n内容: 那这次提前到周四？";
    assert_eq!(context, expected);
}

/// 无历史 / 无记忆时只保留本次消息，空白条目被丢弃
#[test]
fn context_skips_blank_sections() {
    let history = vec!["   ".to_string(), String::new()];
    let context = build_group_context(None, &[], &[], &history, "在吗", "ou_sender", None);
    assert_eq!(context, "【本次消息】\n发送者: ou_sender\n内容: 在吗");
}

/// 超长上下文必须被**模型容量派生**的预算截住，且各段按比例分配
#[test]
fn context_respects_model_capacity_budget() {
    let long = "长".repeat(1000);
    let history: Vec<String> = (0..20).map(|_| format!("用户: {long}")).collect();
    let recent = vec![long.clone(); 10];
    let searched = vec![long.clone(); 10];
    let context = build_group_context(
        None,
        &recent,
        &searched,
        &history,
        &long,
        "ou_sender",
        Some(4_000),
    );
    // 各段份额合计 72%，加标题后仍应明显低于模型容量
    assert!(estimate_tokens(&context) <= 4_000);
    // 历史获得最大份额：应包含群聊段，但不会把记忆段挤掉
    assert!(context.contains("【最近群聊】"));
    assert!(context.contains("【相关记忆】"));
    assert!(context.contains("【最近记忆】"));
    assert!(context.contains("【本次消息】"));
}

/// 容量越大，允许保留的上下文越多（预算确实按模型配置伸缩）
#[test]
fn context_budget_scales_with_model_capacity() {
    let long = "长".repeat(1000);
    let history: Vec<String> = (0..20).map(|_| format!("用户: {long}")).collect();
    let small = build_group_context(None, &[], &[], &history, "在吗", "ou_sender", Some(4_000));
    let large = build_group_context(None, &[], &[], &history, "在吗", "ou_sender", Some(128_000));
    assert!(estimate_tokens(&large) > estimate_tokens(&small));
}

/// 助手身份必须进入上下文：否则消息里点名助手时，小脑不知道该名字指谁
#[test]
fn context_includes_agent_profile() {
    let profile = "【Agent ID】agent_1\n【Agent 名称】前台接待\n【角色描述】接待访客\n【灵魂设定】\n你是「前台接待」";
    let context = build_group_context(
        Some(profile),
        &[],
        &[],
        &[],
        "前台接待，在吗",
        "ou_sender",
        None,
    );
    assert!(
        context.starts_with("【助手身份】\n【Agent ID】agent_1"),
        "agent identity must lead the context, got: {context}"
    );
    assert!(context.contains("【灵魂设定】"));
    assert!(context.contains("【本次消息】"));
}

/// 空白身份不产生「标题 + 空 body」的噪音段落
#[test]
fn blank_agent_profile_is_skipped() {
    let context = build_group_context(Some("   "), &[], &[], &[], "在吗", "ou", None);
    assert!(!context.contains("【助手身份】"));
    assert_eq!(context, "【本次消息】\n发送者: ou\n内容: 在吗");
}

/// 身份段同样受模型容量预算约束（不因新增身份而越过容量）
#[test]
fn agent_profile_respects_capacity_budget() {
    let profile = "长".repeat(5_000);
    let context = build_group_context(
        Some(&profile),
        &[],
        &[],
        &[],
        "在吗",
        "ou_sender",
        Some(4_000),
    );
    assert!(estimate_tokens(&context) <= 4_000);
    assert!(context.contains("【助手身份】"));
}
