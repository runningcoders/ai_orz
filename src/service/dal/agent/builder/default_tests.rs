//! tests 单元测试（拆分自 default.rs）
//!
//! 文件瘦身：原 2090 行 → 1881 行，测试体 210 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::prompt_builder::PromptBuilder;

/// 词表注入预算随 provider 容量变：同一份词表在三种容量下三种结果
///
/// - `None`（容量未配）⇒ 不裁，全量装（与改造前「正常全量注入」等价）
/// - `Some(4_000)` ⇒ 预算 60 token，连段首都装不下 ⇒ 整段让位 + 裁剪提示
/// - `Some(200_000)` ⇒ 预算 3_000 token，20 条全部装下
///
/// 改前这里是固定的 3_000 **字符**，换容量不会变 ⇒ 本用例改前应红（R66 档②）。
#[test]
fn lexicon_budget_follows_provider_capacity() {
    use common::ontology::{LexiconTermSummary, OntologyLexiconSummary};

    let terms: Vec<LexiconTermSummary> = (0..20)
        .map(|i| LexiconTermSummary {
            term_key: format!("term_{i}"),
            display_name: "词条".into(),
            description: "描".repeat(100),
        })
        .collect();
    let lexicon = OntologyLexiconSummary {
        relation_types: terms,
        ..Default::default()
    };

    let render = |capacity: Option<u64>| {
        let mut builder = DefaultPromptBuilder::default();
        builder.set_context_capacity_tokens(capacity);
        builder.ontology_lexicon(&lexicon);
        builder.build_common_context_sections()
    };

    let unset = render(None);
    let small = render(Some(4_000));
    let large = render(Some(200_000));

    assert!(
        !unset.contains("词表超出注入预算"),
        "容量未配 ⇒ 不裁（不猜兜底值）: {unset}"
    );
    assert!(unset.contains("term_19"), "未配置容量时全量装: {unset}");

    assert!(
        small.contains("词表超出注入预算"),
        "4K 容量下 60 token 预算装不下任何一段: {small}"
    );
    assert!(!small.contains("term_0"), "整段让位时一条都不露: {small}");

    assert!(
        !large.contains("词表超出注入预算"),
        "200K 容量下 3_000 token 够装 20 条: {large}"
    );
    assert!(large.contains("term_19"), "预算足够时末条也在: {large}");
    assert!(
        small.chars().count() < large.chars().count(),
        "同一词表按容量换取不同预算: small={} large={}",
        small.chars().count(),
        large.chars().count()
    );
}

/// 意图区块的逐条 / 摘要上限按 token 判，不按字符判
///
/// 同一份内容换语言：拉丁 600 字符 = 150 token（160 预算内，原样留），
/// CJK 400 字符 = 400 token（超预算，被裁）。角色反转的那一对在摘要上：
/// 拉丁 2_000 字符 = 500 token（800 预算内，原样留）、CJK 1_200 字符 = 1_200 token（被裁）。
///
/// 改前是 `chars().take(150)` / `chars().take(800)`：拉丁 600 字符与 CJK 1_200 字符
/// 都会被裁 ⇒ 本用例的两条「拉丁原样留」断言改前应红（R66 档②）。
#[test]
fn intent_section_budget_counts_tokens_not_chars() {
    use crate::service::domain::runtime::awakening::IntentAnalysis;

    let latin_item = "a".repeat(600);
    let cjk_item = "字".repeat(400);
    let item_ia = IntentAnalysis {
        intent_type: "Question".into(),
        confidence: 0.9,
        key_terms: vec![latin_item.clone()],
        retrieved_context: vec![cjk_item.clone()],
        summary: String::new(),
        ..Default::default()
    };
    let render = |capacity: Option<u64>, ia: IntentAnalysis| {
        let mut builder = DefaultPromptBuilder::default();
        builder.set_context_capacity_tokens(capacity);
        builder.intent_analysis = Some(ia);
        builder.render_intent_analysis_section()
    };

    let section = render(Some(200_000), item_ia.clone());
    assert!(
        section.contains(&latin_item),
        "拉丁 600 字符 = 150 token，160 token 预算内不该被裁: {section}"
    );
    assert!(
        !section.contains(&cjk_item),
        "CJK 400 字符 = 400 token，超 160 预算必须被裁: {section}"
    );

    // 容量未配 ⇒ 不裁：同一条 CJK 条目原样保留
    let unset = render(None, item_ia);
    assert!(unset.contains(&cjk_item), "容量未配 ⇒ 不裁: {unset}");

    let summary_ia = |summary: String| IntentAnalysis {
        intent_type: "Question".into(),
        confidence: 0.9,
        summary,
        ..Default::default()
    };
    let latin_summary = "b".repeat(2_000);
    let cjk_summary = "字".repeat(1_200);
    assert!(
        render(Some(200_000), summary_ia(latin_summary.clone())).contains(&latin_summary),
        "拉丁 2_000 字符 = 500 token，800 预算内不该被裁"
    );
    assert!(
        !render(Some(200_000), summary_ia(cjk_summary.clone())).contains(&cjk_summary),
        "CJK 1_200 字符 = 1_200 token，超 800 预算必须被裁"
    );
}

/// 【相关知识】区块必须点名「按 id 反查详情」与「按节点 id 沿边展开」两条路
#[test]
fn retrieved_knowledge_hint_points_to_both_lookup_paths() {
    let mut builder = DefaultPromptBuilder::default();
    builder
        .retrieved_knowledge_lines
        .push("[索引] [st0] 示例行".to_string());
    let section = builder.render_retrieved_knowledge_section();

    assert!(section.contains("query_memory(ids="), "{section}");
    assert!(section.contains("seed_node_ids="), "{section}");
    assert!(section.contains("traversal_depth=1"), "{section}");
    assert!(
        section.contains("[关系 id]") && section.contains("query_memory(ids=[边id])"),
        "边行的反查 / 继续展开方式也要点名: {section}"
    );
    assert!(section.contains("- [索引] [st0] 示例行"), "{section}");
    assert!(
        !section.contains("- - "),
        "行前缀只能在区块里加一次: {section}"
    );
}

/// 没有技能时不拼装提醒：此时提醒读技能是无意义噪声，还会让「请回复」收尾分叉
#[test]
fn skill_recall_nudge_is_empty_without_skills() {
    let builder = DefaultPromptBuilder::default();
    assert!(builder.skill_recall_nudge().is_empty());
}

/// 有技能时必须提醒，且点名 get_skill（技能正文是按需加载，不提醒模型不会去读）
#[test]
fn skill_recall_nudge_points_to_get_skill() {
    let mut builder = DefaultPromptBuilder::default();
    builder.skills.push(SkillPo::default());
    let nudge = builder.skill_recall_nudge();
    assert!(!nudge.is_empty());
    assert!(nudge.contains("get_skill"), "{nudge}");
    assert!(nudge.contains("读全文的时机"), "{nudge}");
}

/// 前缀不变量：意图初始消息的 System / User 必须原样包含 Awaken base，
/// 意图指令只能是同一 user 消息的**尾部追加**（I1/I2/I4）。
#[test]
fn intent_initial_messages_reuse_awaken_base_as_prefix() {
    let mut builder = DefaultPromptBuilder {
        system_prompt: Some("PERSONA".to_string()),
        ..Default::default()
    };
    builder.skills.push(SkillPo::default());
    builder.history.push("H1".to_string());
    builder.current_message = Some("CURRENT".to_string());

    let base_system = builder.awaken_system_part();
    let base_user = builder.awaken_user_part();
    let messages = builder.build_intent_analyze_initial_messages();
    assert_eq!(messages.len(), 2);

    match &messages[0] {
        ChatMessage::System { content } => assert_eq!(content, &base_system),
        _ => panic!("首条必须是 System"),
    }
    match &messages[1] {
        ChatMessage::User { content } => {
            assert!(
                content.starts_with(&base_user),
                "意图指令只能追加在 base 之后"
            );
            assert!(
                content.contains("INTENT_ANALYSIS_START"),
                "缺少意图指令尾部"
            );
        }
        _ => panic!("次条必须是 User"),
    }
}

/// 本轮指引（回复仲裁 + 协作压力）必须 append-only 收尾：插入点之前的所有
/// 字节逐字节不变，且不进 System / 不并进【资源上下文】（prefix cache I1/I2 护栏）。
#[test]
fn guidance_block_is_append_only_and_preserves_stable_prefix() {
    let mut builder = DefaultPromptBuilder {
        system_prompt: Some("PERSONA".to_string()),
        ..Default::default()
    };
    builder.history.push("H1".to_string());
    // 直接给出 current_message 渲染后的文本形态（含【资源上下文】区块），
    // 本护栏测的是 build 层装配顺序，不需要再构造 Message 领域对象
    builder.current_message =
        Some("CURRENT\n\n【资源上下文】\n本条消息引用了以下资源：\n- ATTACHMENT.md\n".to_string());

    let before = builder.awaken_user_part();
    let system = builder.awaken_system_part();

    builder.set_turn_guidance_lines(vec![
        "【协作压力】链深 3，疲劳度 36/100。".to_string(),
        "- 本轮指引：请引入新信息或主动收敛".to_string(),
    ]);
    let after = builder.awaken_user_part();
    let pressure_at = after.find("【协作压力】").expect("压力区块必须渲染");

    assert_eq!(
        &after[..pressure_at],
        &before[..pressure_at],
        "压力块之前的前缀必须逐字节不变，否则跨唤醒缓存命中面被打穿"
    );
    assert!(
        pressure_at > before.find("【资源上下文】").unwrap(),
        "压力块不得并入或前移到【资源上下文】之前"
    );
    assert!(
        pressure_at > before.find("CURRENT").unwrap(),
        "压力块必须在当前消息之后"
    );
    assert!(
        !builder.awaken_system_part().contains("协作压力"),
        "System 是跨唤醒唯一可复用前缀，禁止携带每轮变化的压力值"
    );
    assert_eq!(system, builder.awaken_system_part());
    assert!(
        !after.contains("- 【协作压力】"),
        "压力行是独立区块，不是【资源上下文】的列表项"
    );
    // 收尾指令必须给出「可以回 NO_REPLY」的出口，否则老文案「请回复：」
    // 等于替 Agent 假定必须回，与仲裁区块自相矛盾
    assert!(
        after.ends_with("请输出本轮结果（无需回复时只输出 NO_REPLY）："),
        "有指引时的收尾指令必须暴露 NO_REPLY 出口：{after}"
    );

    // 跨唤醒压力值每轮递增，插入点之前的前缀仍必须逐字节一致
    builder.set_turn_guidance_lines(vec![
        "【协作压力】链深 8，疲劳度 96/100。".to_string(),
        "- 本轮指引：必须收敛".to_string(),
    ]);
    let next = builder.awaken_user_part();
    let next_at = next.find("【协作压力】").unwrap();
    assert_eq!(
        &next[..next_at],
        &before[..next_at],
        "压力值变化不得改动前缀，否则跨唤醒缓存命中面被打穿"
    );
}

/// 带指引行时两阶段仍需逐字节一致：意图指令只能继续在 base 尾部追加。
#[test]
fn guidance_lines_keep_intent_prefix_identical() {
    let mut builder = DefaultPromptBuilder {
        system_prompt: Some("PERSONA".to_string()),
        ..Default::default()
    };
    builder.current_message = Some("CURRENT".to_string());
    builder.set_turn_guidance_lines(vec!["【协作压力】链深 2".to_string()]);

    let base_system = builder.awaken_system_part();
    let base_user = builder.awaken_user_part();
    let messages = builder.build_intent_analyze_initial_messages();
    assert_eq!(messages.len(), 2);
    match (&messages[0], &messages[1]) {
        (ChatMessage::System { content: s }, ChatMessage::User { content: u }) => {
            assert_eq!(s, &base_system);
            assert!(u.starts_with(&base_user), "意图指令只能追加在 base 之后");
            assert!(u.contains("【协作压力】"), "Phase1 需看到同一份压力信号");
        }
        _ => panic!("两阶段首两条必须是 System + User"),
    }
}

/// 用户 ↔ Agent 主流路径零变化：没有本轮指引时，收尾指令逐字节保持原样
#[test]
fn reply_tail_stays_unchanged_without_guidance() {
    let mut builder = DefaultPromptBuilder {
        system_prompt: Some("PERSONA".to_string()),
        ..Default::default()
    };
    builder.current_message = Some("CURRENT".to_string());
    let user = builder.awaken_user_part();
    assert!(user.ends_with("\n\n请回复："), "{user}");
    assert!(
        !user.contains("NO_REPLY"),
        "用户对话不该看到内部哨兵：{user}"
    );
}

/// 执行段（Phase 2）才是真正的决策点：同一份指引与 NO_REPLY 出口必须出现在这里，
/// 否则模型要回头翻 Phase 1 才知道「可以不回」，实际仍会每轮都答。
#[test]
fn execution_segment_carries_turn_guidance_and_reply_tail() {
    let mut builder = DefaultPromptBuilder {
        current_message: Some("CURRENT".to_string()),
        ..Default::default()
    };
    builder.set_turn_guidance_lines(vec![
        "【回复仲裁】本条来自另一个 Agent，在等你答复；是否回复由你决定。".to_string(),
    ]);
    let segment = builder.build_execution_segment();
    assert!(segment.contains("【回复仲裁】"), "{segment}");
    assert!(
        segment.contains("若判断无需回复来源方，只输出 NO_REPLY"),
        "{segment}"
    );
    assert!(
        segment.ends_with("请输出本轮结果（无需回复时只输出 NO_REPLY）："),
        "{segment}"
    );
}
