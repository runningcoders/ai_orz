//! tests 单元测试（拆分自 compaction.rs）
//!
//! 文件瘦身：原 966 行 → 722 行，测试体 245 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

/// 老判据只看首尾文本，长工具链 + 短回复会被整段漏掉
#[test]
fn chain_metrics_capture_long_tool_chain_with_short_reply() {
    let messages = vec![
        ChatMessage::System {
            content: "人设".repeat(2000),
        },
        ChatMessage::user("帮我查一下"),
        ChatMessage::Tool {
            tool_call_id: "t1".into(),
            content: "结果".repeat(700),
        },
        ChatMessage::Tool {
            tool_call_id: "t2".into(),
            content: "结果".repeat(700),
        },
    ];
    let m = measure_turn_chain(&messages, 2, "帮我查一下", "查到了");
    assert_eq!(m.rounds_used, 2);
    assert_eq!(m.tool_calls, 2);
    assert_eq!(m.tool_result_chars, 2800);
    // System 人设 / User 通用上下文是每轮重拼的样板，绝不能计入
    assert!(m.turn_tokens < 20, "首尾文本很短：{m:?}");
}

/// 核心回归：短回复 + 长工具链必须命中链路策略，走 LLM 总结
#[test]
fn compaction_policy_hits_chain_even_when_reply_is_short() {
    let messages = vec![
        ChatMessage::Tool {
            tool_call_id: "t1".into(),
            content: "x".repeat(50),
        },
        ChatMessage::Tool {
            tool_call_id: "t2".into(),
            content: "x".repeat(50),
        },
    ];
    let chain = measure_turn_chain(&messages, 2, "帮我查一下", "查到了");
    let hits = compaction_policy_for(1024).evaluate(&chain.into_policy_metrics());
    assert!(
        hits.iter().any(|h| h == "compaction_chain"),
        "工具链应命中链路策略，实际 hits={hits:?}"
    );
    assert!(!hits.iter().any(|h| h == "compaction_threshold"));
}

/// 短闲聊仍走 quick_turn，不白跑一次 LLM
#[test]
fn compaction_policy_keeps_short_chit_chat_on_quick_turn() {
    let chain = measure_turn_chain(&[], 0, "你好", "你好，我是助手");
    assert!(
        compaction_policy_for(1024)
            .evaluate(&chain.into_policy_metrics())
            .is_empty()
    );
}

/// 极长回复仍由文本维度兜住（调大阈值不等于关掉压缩）
#[test]
fn compaction_policy_still_hits_on_very_long_reply() {
    let chain = measure_turn_chain(&[], 0, "请写一篇长文", &"很长".repeat(600));
    let hits = compaction_policy_for(1024).evaluate(&chain.into_policy_metrics());
    assert!(hits.iter().any(|h| h == "compaction_threshold"));
}

/// 多轮思考本身就是「有中间过程」的信号
#[test]
fn compaction_policy_hits_on_many_rounds_without_tools() {
    let chain = measure_turn_chain(&[], 3, "再想想", "好了");
    assert_eq!(chain.tool_calls, 0);
    let hits = compaction_policy_for(1024).evaluate(&chain.into_policy_metrics());
    assert!(hits.iter().any(|h| h == "compaction_chain"));
}

/// 压缩与总结必须分场景：轨迹 / AOP 统计靠 scene 区分，不能都记成 compact
#[test]
fn compact_mode_binds_scene_and_reply_form() {
    assert_eq!(CompactMode::Overflow.scene(), ThinkingScene::Compact);
    assert_eq!(CompactMode::TurnArchive.scene(), ThinkingScene::Summary);
    assert_eq!(CompactMode::ForcedReply.scene(), ThinkingScene::Summary);
    // 只有轮次耗尽才需要给用户的收尾回复
    assert!(CompactMode::ForcedReply.with_user_reply());
    assert!(!CompactMode::Overflow.with_user_reply());
    assert!(!CompactMode::TurnArchive.with_user_reply());
}

#[test]
fn compact_instruction_forbids_compressing_boilerplate() {
    let s = compact_instruction(false);
    // 明确只压缩「真正干的活儿」
    assert!(s.contains("只压缩"));
    assert!(s.contains("真正干的活儿"));
    // 逐项排除每次都会重新注入的内容
    assert!(s.contains("人设"));
    assert!(s.contains("技能"));
    assert!(s.contains("【用户画像】"));
    assert!(s.contains("【项目上下文】"));
    assert!(s.contains("【任务上下文】"));
    assert!(s.contains("【工作空间与路径约定】"));
    assert!(s.contains("【历史对话】"));
    // 给出可执行的判断标准
    assert!(s.contains("下一轮还会原样出现"));
    // 框架落库：不再要求模型调工具
    assert!(s.contains("不要调用任何工具"));
    assert!(!s.contains("save_short_term_memory 写入"));
    // 要求用标记包裹输出
    assert!(s.contains(SUMMARY_OPEN));
    assert!(s.contains(SUMMARY_CLOSE));
}

#[test]
fn compact_instruction_with_user_reply_demands_marker() {
    let s = compact_instruction(true);
    assert!(s.contains(USER_REPLY_OPEN));
    assert!(s.contains(USER_REPLY_CLOSE));
    assert!(s.contains("直接发送给用户"));
    // 无回复场景不该出现回复标记
    assert!(!compact_instruction(false).contains(USER_REPLY_OPEN));
}

/// 轻量落档必须同时留下「用户说了什么」和「助手回了什么」
///
/// 它是跳过压缩后下一轮唤醒能看到的唯一上下文，任一侧丢失都会断片。
#[test]
fn quick_turn_summary_keeps_both_sides() {
    let s = build_quick_turn_summary("  你好  ", " 你好，我是助手 ").expect("非空");
    assert_eq!(s, "用户：你好\n助手：你好，我是助手");
}

/// 阈值内的短对话保留**全文**，不做任何截断
///
/// 这是「按长度分流」的核心承诺：短内容就是原文记账，
/// 一旦仍按单侧 200 字截断，用户说的后半句就丢了，等于白记。
#[test]
fn quick_turn_summary_keeps_short_turn_verbatim() {
    let user: String = (0..300).map(|_| '问').collect();
    let reply: String = (0..80).map(|_| '答').collect();
    let s = build_quick_turn_summary(&user, &reply).expect("非空");
    assert_eq!(s, format!("用户：{user}\n助手：{reply}"));
    assert!(!s.ends_with('…'), "阈值内不应截断: {s}");
}

/// 超过硬上限的极长内容两侧均分截断兜底
///
/// 只有直答/澄清（没有 messages 可压缩）才可能走到这里；兜底是为了
/// 不让一条原文长期占用后续 ~20 轮 prompt 预算。
#[test]
fn quick_turn_summary_caps_extremely_long_turn() {
    let long: String = (0..QUICK_TURN_HARD_CAP_CHARS + 50).map(|_| '字').collect();
    let s = build_quick_turn_summary(&long, "").expect("非空");
    assert!(s.starts_with("用户："));
    assert!(s.ends_with('…'), "超硬上限应带省略号: {s}");
    assert!(!s.contains('\n'), "没有回复时不应出现助手行");
    // 截断按字符而非字节：中文不能切在半个字上
    assert_eq!(
        s.chars().count(),
        "用户：".chars().count() + QUICK_TURN_HARD_CAP_CHARS / 2 + 1
    );
}

#[test]
fn quick_turn_summary_rejects_all_empty() {
    assert!(build_quick_turn_summary("   ", "\n").is_none());
}

/// 中文按 1 token/字，英文按 4 字符/token，混合文本逐类累加
#[test]
fn estimate_tokens_counts_cjk_and_latin_separately() {
    // 纯中文：一字一 token
    assert_eq!(estimate_tokens("你好世界"), 4);
    // 纯英文：4 字符一 token（向上取整）
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcde"), 2);
    // 混合：5 汉字 + 8 拉丁字符 = 5 + 2
    assert_eq!(estimate_tokens("你好世界啊abcdefgh"), 7);
    // 空白不特殊处理，仍按拉丁字符计入
    assert_eq!(estimate_tokens("   "), 1);
    assert_eq!(estimate_tokens(""), 0);
}

/// 分流依据是「用户 + 助手」的合计，不是单侧
///
/// 反例：2000 字报错 + 一句「改第 42 行」，只看回复会被误判成短对话。
#[test]
fn estimate_turn_tokens_sums_both_sides() {
    assert_eq!(estimate_turn_tokens("你好", "世界"), 4);
    assert_eq!(estimate_turn_tokens("  ", "  "), 0);
}

#[test]
fn parse_compacted_summary_extracts_marked_block() {
    let text =
        format!("前面一段话\n{SUMMARY_OPEN}\n完成了 A，待办是 B\n{SUMMARY_CLOSE}\n后面一段话");
    assert_eq!(
        parse_compacted_summary(&text),
        Some("完成了 A，待办是 B".to_string())
    );
}

#[test]
fn parse_compacted_summary_tolerates_missing_close() {
    // 输出被截断时至少保住开头到结尾之间的内容
    let text = format!("{SUMMARY_OPEN}\n完成了 A");
    assert_eq!(parse_compacted_summary(&text), Some("完成了 A".to_string()));
}

#[test]
fn parse_compacted_summary_returns_none_when_absent_or_empty() {
    assert_eq!(parse_compacted_summary("没有标记的一段话"), None);
    let empty = format!("{SUMMARY_OPEN}\n   \n{SUMMARY_CLOSE}");
    assert_eq!(parse_compacted_summary(&empty), None);
}

#[test]
fn extract_user_reply_takes_marked_sentence_only() {
    let text = format!(
        "{SUMMARY_OPEN}\n内部摘要：已完成 A，待办 B\n{SUMMARY_CLOSE}\n\
         {USER_REPLY_OPEN}\n已完成三件事，因轮次耗尽停止。\n{USER_REPLY_CLOSE}"
    );
    assert_eq!(
        extract_user_reply(&text),
        Some("已完成三件事，因轮次耗尽停止。".to_string())
    );
}

/// 最关键的一条防线：模型漏打任何标记时，**绝不能**把内部摘要当回复
///
/// 旧实现 `strip_summary_block` 在无标记时原样返回整段，用户会收到
/// 「已完成的工作 / 还没做完的待办」这种内部口吻的摘要。
#[test]
fn extract_user_reply_never_leaks_summary() {
    // 只有摘要、没有回复标记
    let summary_only = format!("{SUMMARY_OPEN}\n已完成 A，待办 B\n{SUMMARY_CLOSE}");
    assert_eq!(extract_user_reply(&summary_only), None);

    // 完全没标记：无法区分是「纯摘要」还是「纯回复」，一律不发给用户
    assert_eq!(extract_user_reply("已完成 A，待办 B"), None);

    // 标记内为空
    let empty = format!("{USER_REPLY_OPEN}\n  \n{USER_REPLY_CLOSE}");
    assert_eq!(extract_user_reply(&empty), None);
}
