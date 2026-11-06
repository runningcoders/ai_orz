//! tests 单元测试（拆分自 memory.rs）
//!
//! 文件瘦身：原 1013 行 → 770 行，测试体 244 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use common::enums::MemoryStatus;

fn node(summary: &str) -> Memory {
    Memory::new(MemoryPo::KnowledgeNode(LongTermKnowledgeNodePo {
        id: "kn_1".to_string(),
        agent_id: "agent-a".to_string(),
        node_name: "订单状态机".to_string(),
        node_description: "订单状态机描述订单从创建到完成的流转。".to_string(),
        node_type: "general".to_string(),
        summary: summary.to_string(),
        tags: "[]".to_string(),
        user_tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    }))
}

fn relation(kind: &str) -> Memory {
    Memory::new(MemoryPo::Relation(KnowledgeNodeRelationPo {
        id: "kr_1".to_string(),
        source_node_id: "kn_a".to_string(),
        target_node_id: "kn_b".to_string(),
        relation_type: kind.to_string(),
        // 未标注示例：没有强度时不能写成 0.0（那是「明确很弱」）
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: 0,
        updated_at: 0,
    }))
}

/// 关系类型：DTO 透出**原文**，展示走词表映射（词表外原样）。
///
/// 回归 1：`format!("{:?}")` 会输出 Rust 变体名 `"Causes"`，前端查不到中文
/// 而原样显示英文 —— 画布上所有连线标签、详情面板「内容」都是这串英文。
/// 回归 2：写入方标注的词表外原文（如「实现」）**必须**原样透出，不能因为
/// 匹配不上就归一成 `Custom` / 显示成「自定义」—— 那会把对方明确的语义抹掉。
#[test]
fn relation_type_keeps_raw_text_and_maps_label() {
    let dto = relation("causes").to_api_result();
    assert_eq!(dto.relation_type.as_deref(), Some("causes"));
    assert_eq!(dto.content, "导致", "关系边的内容应是人可读的中文标签");
    assert_eq!(dto.name.as_deref(), Some("导致"));

    let dto = relation("contained_by").to_api_result();
    assert_eq!(dto.relation_type.as_deref(), Some("contained_by"));
    assert_eq!(dto.content, "属于");

    // 词表外：原文与展示标签都必须保留
    let dto = relation("实现").to_api_result();
    assert_eq!(dto.relation_type.as_deref(), Some("实现"));
    assert_eq!(dto.content, "实现");
    assert_eq!(dto.name.as_deref(), Some("实现"));

    // 空白类型：不给空标签（空 tag 会让画布 hover 失效），回退「关联」
    let dto = relation("   ").to_api_result();
    assert_eq!(dto.relation_type, None);
    assert_eq!(dto.content, "关联");
}

/// 空摘要归一成 `None`：前端 `if let Some(summary)` 否则会渲染一个空块。
#[test]
fn blank_summary_is_none() {
    assert!(node("").to_api_result().summary.is_none());
    assert!(node("   ").to_api_result().summary.is_none());
    assert_eq!(
        node("独立摘要").to_api_result().summary.as_deref(),
        Some("独立摘要")
    );
}

/// `score` 是**相关度**（越大越相关），由向量距离换算而来。
#[test]
fn score_is_relevance_not_distance() {
    let memory = node("").with_search_match(SearchMatchInfo {
        match_type: MatchType::Vector,
        vector_distance: Some(0.2),
        ..Default::default()
    });
    let dto = memory.to_api_result();
    let score = dto.score.expect("向量命中应有相关度");
    assert!((score - 0.8).abs() < 1e-6, "距离 0.2 应换算成相关度 0.8");
    assert_eq!(
        dto.search_match.as_ref().map(|m| m.match_type.as_str()),
        Some("vector"),
        "匹配方式要一并暴露，否则用户分不清语义还是关键词命中"
    );
}

/// `published` 是「重要性/影响力」标记（与 `is_published` 冗余字段同源），不是业务标签，
/// 回给前端会在卡片上多出一个没有意义的英文胶囊。它不承载可见性语义（蜂巢全域可见）。
#[test]
fn published_control_tag_is_not_exposed_as_label() {
    let mut memory = node("");
    if let MemoryPo::KnowledgeNode(kn) = &mut memory.po {
        // 存量脏值形态：迁移只搬列不清洗，`published` 同时躺在 tags 与 user_tags 里。
        // 读侧必须仍然只吐业务标签（published 的真相源是 is_published）。
        kn.tags = r#"["published","架构"]"#.to_string();
        kn.user_tags = r#"["published","架构"]"#.to_string();
    }
    assert_eq!(
        memory.to_api_result().tags,
        Some(vec!["架构".to_string()]),
        "只保留业务标签"
    );
}

/// 没有匹配过程（图谱遍历展开）就没有相关度，前端据此整行不渲染。
#[test]
fn no_match_means_no_score() {
    let dto = node("").to_api_result();
    assert!(dto.score.is_none());
    assert!(dto.search_match.is_none());
}

#[test]
fn property_new_splits_value_into_assigned_column() {
    let po = KnowledgePropertyPo::new(
        PropertyTargetType::Node,
        "node-1",
        "confidence",
        PropertyValue::Number(0.8),
        0,
    );
    assert_eq!(po.target_type, "node");
    assert_eq!(po.target_id, "node-1");
    assert_eq!(po.term_key, "confidence");
    assert_eq!(po.data_type, "number");
    assert_eq!(po.value_num, Some(0.8));
    // 未被 data_type 指派的列必须留 NULL，不残留脏值
    assert!(po.value_text.is_none() && po.value_bool.is_none() && po.value_ref.is_none());
    assert!(po.is_active());
    assert!(!po.id.is_empty());
}

/// 泛化表靠 target_type 解释 target_id：边属性必须落 "relation"，
/// 否则按节点查会静默查不到（无外键兜底，拼错不报错）。
#[test]
fn property_relation_target_is_distinct_from_node() {
    let edge = KnowledgePropertyPo::new(
        PropertyTargetType::Relation,
        "rel-1",
        "effective_from",
        PropertyValue::Date("2026-10-11".into()),
        0,
    );
    assert_eq!(edge.target_type, "relation");
    assert_eq!(edge.target(), PropertyTargetType::Relation);
    // 同一 term_key 挂在不同载体上是两条独立记录，互不覆盖
    let node = KnowledgePropertyPo::new(
        PropertyTargetType::Node,
        "rel-1",
        "effective_from",
        PropertyValue::Date("2026-10-11".into()),
        0,
    );
    assert_ne!(edge.target_type, node.target_type);
}

#[test]
fn property_dirty_target_type_falls_back_to_node() {
    let po = KnowledgePropertyPo {
        target_type: "nodes".to_string(),
        ..KnowledgePropertyPo::new(
            PropertyTargetType::Node,
            "node-1",
            "p",
            PropertyValue::Text("v".into()),
            0,
        )
    };
    assert_eq!(
        po.target(),
        PropertyTargetType::Node,
        "脏载体名软门禁退 node，不 panic"
    );
}

#[test]
fn property_boolean_false_is_distinct_from_unset() {
    // false 落 0，未赋值落 NULL —— 两者在读侧必须可区分
    let po = KnowledgePropertyPo::new(
        PropertyTargetType::Node,
        "node-1",
        "enabled",
        PropertyValue::Boolean(false),
        0,
    );
    assert_eq!(po.value_bool, Some(0));
    assert_eq!(po.value(), Some(PropertyValue::Boolean(false)));
}

#[test]
fn property_value_roundtrip_all_types() {
    let cases = [
        PropertyValue::Text("t".into()),
        PropertyValue::Number(0.5),
        PropertyValue::Boolean(true),
        PropertyValue::Date("2026-10-11".into()),
        PropertyValue::Reference("node-9".into()),
        PropertyValue::Enum("open".into()),
    ];
    for value in cases {
        let po =
            KnowledgePropertyPo::new(PropertyTargetType::Node, "node-1", "p", value.clone(), 0);
        assert_eq!(po.value(), Some(value));
    }
}

#[test]
fn property_missing_assigned_column_reads_as_none() {
    // 声明 number 但数值列为 NULL = 脏数据行，不猜 0 不 panic
    let po = KnowledgePropertyPo {
        id: "p1".to_string(),
        target_type: "node".to_string(),
        target_id: "node-1".to_string(),
        term_key: "confidence".to_string(),
        data_type: "number".to_string(),
        ordinal: 0,
        value_text: None,
        value_num: None,
        value_bool: None,
        value_ref: None,
        status: KnowledgeRelationStatus::Active,
        created_at: 1,
        updated_at: 1,
    };
    assert_eq!(po.value(), None);
}

#[test]
fn property_superseded_and_deleted_are_not_active() {
    let mut po = KnowledgePropertyPo::new(
        PropertyTargetType::Node,
        "node-1",
        "p",
        PropertyValue::Text("v".into()),
        0,
    );
    assert!(po.is_active());
    po.status = KnowledgeRelationStatus::Superseded;
    assert!(!po.is_active());
    po.status = KnowledgeRelationStatus::Deleted;
    assert!(!po.is_active());
}

/// B 单 CP1（B-e'）：知识节点三种存储状态 → 回包 `status` 文案。
///
/// `Forgotten`(=0) 回包写 `retired`，而**请求参数侧仍写 `forgotten`** ——
/// 同义不同名（存储叫「已遗忘」、回包叫「已退役」），不要当成第五种状态。
#[test]
fn status_label_maps_every_node_status_value() {
    for (status, want) in [
        (MemoryStatus::Active, "active"),
        (MemoryStatus::Settled, "settled"),
        (MemoryStatus::Forgotten, "retired"),
    ] {
        let mut memory = node("摘要");
        if let MemoryPo::KnowledgeNode(po) = &mut memory.po {
            po.status = status;
        }
        assert_eq!(
            memory.to_api_result().status.as_deref(),
            Some(want),
            "知识节点 {status:?} 必须回显 {want}"
        );
    }
}

/// B 单 CP1：关系边三种存储状态 → 回包 `status` 文案。
///
/// `Deleted`(=2) 必须显式给出：默认检索只排除 `status = 0`，被删边仍会进结果，
/// 合并成 `active` 会让消费方看不出「这条已被删过」。
#[test]
fn status_label_maps_every_relation_status_value() {
    for (status, want) in [
        (KnowledgeRelationStatus::Active, "active"),
        (KnowledgeRelationStatus::Superseded, "retired"),
        (KnowledgeRelationStatus::Deleted, "deleted"),
    ] {
        let mut memory = relation("causes");
        if let MemoryPo::Relation(po) = &mut memory.po {
            po.status = status;
        }
        assert_eq!(
            memory.to_api_result().status.as_deref(),
            Some(want),
            "关系边 {status:?} 必须回显 {want}"
        );
    }
}

/// B 单 CP1（旧形状兼容）：`status = None` 时序列化载荷里**整键缺席**。
///
/// short_term / trace 没有状态维度；空串或 null 都会被前端读成
/// 「有状态但没值」，只有键缺席才是「该类型无此维度」。
#[test]
fn status_key_is_absent_from_payload_when_none() {
    let result =
        crate::models::memory::Memory::new(MemoryPo::KnowledgeNode(LongTermKnowledgeNodePo {
            id: "kn_absent".to_string(),
            agent_id: "agent-a".to_string(),
            node_name: "节点".to_string(),
            node_description: "正文".to_string(),
            node_type: "general".to_string(),
            summary: String::new(),
            tags: "[]".to_string(),
            user_tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: 0,
            updated_at: 0,
        }))
        .to_api_result();
    assert_eq!(result.status.as_deref(), Some("active"));

    let mut absent = result;
    absent.status = None;
    let json = serde_json::to_value(&absent).unwrap();
    assert!(
        !json.as_object().unwrap().contains_key("status"),
        "status = None 时不得出现该键（不是 null、不是空串）: {json}"
    );
}

// ==================== 记忆索引分层 ====================

/// 固定绝对时间戳：测试不能依赖本机当前时刻
const FIXED_TS: i64 = 1_762_400_000;

fn short_term(summary: &str) -> Memory {
    Memory::new(MemoryPo::ShortTerm(ShortTermMemoryIndexPo {
        id: "m_8f3a2b1".to_string(),
        agent_id: "agent-a".to_string(),
        task_id: Some("t_12".to_string()),
        role: "assistant".to_string(),
        summary: summary.to_string(),
        tags: r#"["缓存","前缀"]"#.to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: FIXED_TS,
        updated_at: FIXED_TS,
    }))
}

/// 600 字符：头 200「甲」/ 中 200「乙」/ 尾 200「丙」，三段各自可辨识
fn segmented_text() -> String {
    format!(
        "{}{}{}",
        "甲".repeat(200),
        "乙".repeat(200),
        "丙".repeat(200)
    )
}

/// 补全后的全文行必须带反查把手（id）、绝对时间、tags 与 task_id。
/// 无 T0 锚点时不标注相对时间（只有索引块走 `to_index_prompt_at` 才标注），
/// 避免 5 条参考块里出现没有基准的「N 天前」。
#[test]
fn prompt_summary_carries_metadata_and_absolute_time() {
    let line = short_term("讨论了前缀缓存的分层方案")
        .to_prompt_summary()
        .expect("短期记忆应有全文行");
    assert!(line.starts_with("[m_8f3a2b1] "), "id 是反查把手: {line}");
    assert!(
        line.contains("#缓存") && line.contains("#前缀"),
        "tags: {line}"
    );
    assert!(line.contains("task=t_12"), "任务线索: {line}");
    assert!(
        line.contains(&format_absolute_time(FIXED_TS)),
        "可读绝对时间: {line}"
    );
    assert!(
        line.contains(&format!("ts={FIXED_TS}")),
        "机器可读的 Unix 秒级时间戳（过滤把手）: {line}"
    );
    assert!(
        !line.contains("天前") && !line.contains("小时前") && !line.contains("刚刚"),
        "禁止相对时长: {line}"
    );
    assert!(
        line.ends_with("讨论了前缀缓存的分层方案"),
        "全文不截断: {line}"
    );
}

/// 全文档 = 补全元信息后的原方法，逐字节等价（不能另拼一套格式）
#[test]
fn full_tier_is_byte_equal_to_prompt_summary() {
    let memory = short_term("全文内容 A");
    assert_eq!(
        memory.to_index_prompt(MemoryIndexTier::Full, 64),
        memory.to_prompt_summary()
    );
}

#[test]
fn three_parts_keeps_head_middle_tail_in_order() {
    let line = short_term(&segmented_text())
        .to_index_prompt(MemoryIndexTier::ThreeParts, 64)
        .unwrap();
    assert!(
        line.contains('甲') && line.contains('乙') && line.contains('丙'),
        "{line}"
    );
    assert_eq!(line.matches('…').count(), 2, "三段之间两个省略标记: {line}");
    assert!(
        line.find('甲') < line.find('乙') && line.find('乙') < line.find('丙'),
        "段序必须是前→中→后: {line}"
    );
    assert!(
        line.starts_with("[m_8f3a2b1] "),
        "元信息不随摘要衰减: {line}"
    );
}

#[test]
fn two_parts_and_one_part_drop_expected_segments() {
    let memory = short_term(&segmented_text());

    let two = memory
        .to_index_prompt(MemoryIndexTier::TwoParts, 32)
        .unwrap();
    assert!(two.contains('甲') && two.contains('丙'), "开头+结尾: {two}");
    assert!(!two.contains('乙'), "2 段不保留中部: {two}");
    assert_eq!(two.matches('…').count(), 1, "{two}");

    let one = memory
        .to_index_prompt(MemoryIndexTier::OnePart, 24)
        .unwrap();
    assert!(one.contains('甲'), "仅保留开头: {one}");
    assert!(
        !one.contains('乙') && !one.contains('丙'),
        "1 段不保留中尾: {one}"
    );
    assert!(one.contains('…'), "标明有省略: {one}");
    assert!(one.starts_with("[m_8f3a2b1] "), "1 段仍可反查: {one}");
}

/// 短摘要不该被切成带着省略号的碎片；系数为 0 也退化为全文
#[test]
fn short_summary_and_zero_coefficient_fall_back_to_full() {
    let memory = short_term("短摘要只有一句话");
    for tier in [
        MemoryIndexTier::Full,
        MemoryIndexTier::ThreeParts,
        MemoryIndexTier::TwoParts,
        MemoryIndexTier::OnePart,
    ] {
        let line = memory.to_index_prompt(tier, 24).unwrap();
        assert!(line.ends_with("短摘要只有一句话"), "{tier:?}: {line}");
        assert!(!line.contains('…'), "{tier:?} 不该出现省略号: {line}");
    }
    // coefficient = 0 只表示「不切段」；元信息四档一致，因此等同全文行
    let long = short_term(&segmented_text());
    assert_eq!(
        long.to_index_prompt(MemoryIndexTier::OnePart, 0),
        long.to_prompt_summary()
    );
}

/// 元信息不分级：id / 绝对时间 / tags / task_id 在四档里逐字节相同，
/// 只有摘要正文衰减。删掉远档元信息会让模型失去消歧与反查线索。
#[test]
fn metadata_is_identical_across_all_tiers() {
    let memory = short_term(&segmented_text());
    let full = memory.to_prompt_summary().unwrap();
    let meta_end = full.find(" 甲").expect("全文行里元信息后有空格 + 摘要");
    let expected_meta = &full[..meta_end];

    for tier in [
        MemoryIndexTier::Full,
        MemoryIndexTier::ThreeParts,
        MemoryIndexTier::TwoParts,
        MemoryIndexTier::OnePart,
    ] {
        let line = memory.to_index_prompt(tier, tier.segment_chars()).unwrap();
        assert!(
            line.starts_with(expected_meta),
            "{tier:?} 元信息必须与全文档一致: {line}"
        );
        assert!(line.contains("#缓存") && line.contains("#前缀"), "{tier:?}");
        assert!(line.contains("task=t_12"), "{tier:?}");
        assert!(
            line.contains(&format_absolute_time(FIXED_TS)),
            "{tier:?} 绝对时间不可省: {line}"
        );
    }
}

/// 中文 + emoji 必须在 char 边界切，不能 panic 也不能产出乱码
#[test]
fn segmentation_is_utf8_safe() {
    let text = format!(
        "{}{}🙂{}",
        "中文".repeat(200),
        "中部".repeat(200),
        "结尾".repeat(200)
    );
    let line = short_term(&text)
        .to_index_prompt(MemoryIndexTier::ThreeParts, 64)
        .unwrap();
    assert!(
        line.chars().count() < text.chars().count(),
        "确实压缩了: {line}"
    );
    assert!(
        line.is_char_boundary(line.len()),
        "String 天然保证 UTF-8 完整"
    );
    assert!(line.contains("[m_8f3a2b1] "));
}

/// 只有「有正文的实体」才出行：知识节点已补对称打包，关系边 / 空正文仍为 None
#[test]
fn unsupported_types_and_blank_bodies_are_none() {
    let knowledge = node("知识节点")
        .to_index_prompt(MemoryIndexTier::Full, 64)
        .expect("知识节点应产出索引行");
    assert!(
        knowledge.contains("[知识节点 kn_1]"),
        "查边把手恒在行首: {knowledge}"
    );

    assert!(
        relation("causes")
            .to_index_prompt(MemoryIndexTier::OnePart, 24)
            .is_none()
    );
    assert!(
        short_term("   ")
            .to_index_prompt(MemoryIndexTier::Full, 64)
            .is_none()
    );
    let mut blank = node("");
    if let MemoryPo::KnowledgeNode(k) = &mut blank.po {
        k.node_description.clear();
    }
    assert!(blank.to_index_prompt(MemoryIndexTier::Full, 64).is_none());
}

/// 脑容量：真实压缩阈值 × 10%，未配置走名义基准，极小窗口有预算兜底
#[test]
fn budget_is_ten_percent_of_context_threshold() {
    assert_eq!(
        memory_index_budget_tokens(MEMORY_INDEX_NOMINAL_CONTEXT_TOKENS),
        2_000,
        "20k × 10%"
    );
    assert_eq!(memory_index_budget_tokens(8_000), 800);
    assert_eq!(memory_index_budget_tokens(128_000), 12_800);
    assert_eq!(
        memory_index_budget_tokens(0),
        2_000,
        "未配置时退化为名义基准，绝不能算出 0 预算"
    );
    assert_eq!(
        memory_index_budget_tokens(1_000),
        MEMORY_INDEX_MIN_BUDGET_TOKENS,
        "极小窗口模型仍要有可用索引"
    );
}

/// 四档配额守恒且近档优先：4:3:2:1，余数补近档
#[test]
fn tier_quotas_sum_to_budget_and_favor_recent() {
    assert_eq!(MEMORY_INDEX_TIER_WEIGHTS, [4, 3, 2, 1]);
    for budget in [500, 800, 2_000, 12_800] {
        let quotas = memory_index_tier_quotas(budget);
        assert_eq!(
            quotas.iter().sum::<usize>(),
            budget,
            "配额之和必须等于预算: budget={budget} quotas={quotas:?}"
        );
        assert!(
            quotas.windows(2).all(|w| w[0] >= w[1]),
            "越近配额越多: budget={budget} quotas={quotas:?}"
        );
    }
}

/// 常量自洽：时间戳归一化（消息毫秒 / 记忆秒）不能有第二套口径
#[test]
fn timestamp_normalization_is_shared() {
    assert_eq!(normalize_timestamp_secs(FIXED_TS), FIXED_TS);
    assert_eq!(normalize_timestamp_secs(FIXED_TS * 1000), FIXED_TS);
    assert_eq!(MEMORY_INDEX_DAY_SECS, 86_400);
}

/// 构造距 FIXED_TS 往前 `days` 天的短期记忆（时间正序：下标越大越新）
fn short_term_days_ago(days: i64, summary: &str) -> Memory {
    let mut memory = short_term(summary);
    if let MemoryPo::ShortTerm(st) = &mut memory.po {
        st.created_at = FIXED_TS - days * MEMORY_INDEX_DAY_SECS;
    }
    memory
}

fn count_tiers(plan: &MemoryIndexPlan) -> [usize; 4] {
    let mut counts = [0usize; 4];
    for tier in plan.tiers.iter().flatten() {
        counts[tier.ordinal()] += 1;
    }
    counts
}

/// 造一批有正文体积的候选；单测用字符数计价（生产注入 estimate_tokens）
fn bulk_memories(n: usize) -> Vec<Memory> {
    (0..n)
        .map(|i| short_term_days_ago(0, &format!("第{i}条：{}", "讨论".repeat(30))))
        .collect()
}

fn char_cost(line: &str) -> usize {
    line.chars().count()
}

/// 核心：从最新往最旧按字符数累加，`4` 份配额用满切 `3` 份，以此类推；
/// 档位随新近度单调，四档都会被用到。
#[test]
fn plan_cascades_by_accumulated_line_cost() {
    let memories = bulk_memories(60);
    let plan = MemoryIndexPlan::build(&memories, Some(FIXED_TS), 2_000, char_cost);
    let counts = count_tiers(&plan);
    assert!(counts.iter().all(|&c| c > 0), "四档都应被用到: {counts:?}");
    assert_eq!(
        plan.tier_at(59),
        Some(MemoryIndexTier::Full),
        "最新一条全文"
    );

    // 时间正序（旧→新）：ordinal 只能递减或持平
    let ordinals: Vec<_> = plan.tiers.iter().flatten().map(|t| t.ordinal()).collect();
    assert!(
        ordinals.windows(2).all(|w| w[0] >= w[1]),
        "越近越高保真: {ordinals:?}"
    );
    assert_eq!(plan.anchor_ts, Some(FIXED_TS));
}

/// 不设条数上限：预算够大就全进，预算小就累加到耗尽为止；
/// 预算耗尽是「停止翻更早的记忆」，保留区间必须连续，不能中间挖洞。
#[test]
fn plan_has_no_fixed_count_cap() {
    let memories = bulk_memories(60);
    let small = MemoryIndexPlan::build(&memories, Some(FIXED_TS), 500, char_cost);
    let large = MemoryIndexPlan::build(&memories, Some(FIXED_TS), 20_000, char_cost);
    let kept_small = small.tiers.iter().flatten().count();
    let kept_large = large.tiers.iter().flatten().count();
    assert_eq!(kept_large, 60, "预算充足时 60 条全部进块，无条数上限");
    assert!(kept_small < kept_large, "小预算必须裁掉更早的记忆");
    assert!(
        small.tiers.iter().any(Option::is_none),
        "预算耗尽后更早的记忆为 None"
    );
    let first_kept = small.tiers.iter().position(Option::is_some).unwrap();
    assert!(
        small.tiers[first_kept..].iter().all(Option::is_some),
        "保留区间必须是从最新起连续的一段"
    );
}

/// 成本函数可注入：同一预算下，计价越贵保留越少；单行超所有配额时只保底最新一条
#[test]
fn plan_respects_injected_cost_function() {
    let memories = bulk_memories(20);
    let cheap = MemoryIndexPlan::build(&memories, Some(FIXED_TS), 1_000, |_| 1);
    let expensive = MemoryIndexPlan::build(&memories, Some(FIXED_TS), 1_000, |_| 1_000);
    assert!(cheap.tiers.iter().flatten().count() > expensive.tiers.iter().flatten().count());
    assert!(
        expensive.tiers.iter().flatten().count() <= 1,
        "单行超所有配额时只保底落地最新一条"
    );
}

/// 时间不参与分档：40 天前的记忆不因年龄降级；T0 只做冻结锚点
#[test]
fn plan_freezes_at_t0_and_age_does_not_decide_tier() {
    let mut memories = vec![
        short_term_days_ago(40, "很老"),
        short_term_days_ago(10, "较老"),
        short_term_days_ago(0, "今天"),
    ];
    let mut future = short_term("未来");
    if let MemoryPo::ShortTerm(s) = &mut future.po {
        s.created_at = FIXED_TS + MEMORY_INDEX_DAY_SECS;
    }
    memories.push(future);

    let plan = MemoryIndexPlan::build(&memories, Some(FIXED_TS), 20_000, char_cost);
    assert_eq!(plan.tier_at(3), None, "T0 之后的新记忆不能进冻结索引块");
    assert_eq!(plan.tier_at(2), Some(MemoryIndexTier::Full), "最新一条全文");
    assert_eq!(
        plan.tier_at(0),
        Some(MemoryIndexTier::Full),
        "40 天前也不能因年龄被降级或丢弃"
    );
    // T0 用毫秒传入结果必须一致（消息 created_at 是毫秒，记忆是秒）
    assert_eq!(
        plan,
        MemoryIndexPlan::build(&memories, Some(FIXED_TS * 1000), 20_000, char_cost)
    );
}

/// 相对时间标签：以 T0 为锚点的离散天数桶，不做分钟级漂移
#[test]
fn relative_age_label_is_discrete_and_anchor_based() {
    let cases = [
        (0, "当天"),
        (1, "昨天"),
        (2, "2天前"),
        (7, "7天前"),
        (30, "30天前"),
        (31, "1个月前"),
        (90, "3个月前"),
    ];
    for (days, expected) in cases {
        let created = FIXED_TS - days * MEMORY_INDEX_DAY_SECS;
        assert_eq!(
            relative_age_label(created, FIXED_TS).as_deref(),
            Some(expected),
            "days={days}"
        );
    }
    assert_eq!(
        relative_age_label(FIXED_TS + MEMORY_INDEX_DAY_SECS, FIXED_TS),
        None,
        "T0 之后的未来记忆不标注"
    );
}

/// 索引行元信息：绝对时间与相对时间并存、四档一致；无锚点场景不标注
#[test]
fn index_line_carries_relative_and_absolute_time() {
    let memory = short_term("内容");
    let line = memory
        .to_index_prompt_at(MemoryIndexTier::Full, 64, Some(FIXED_TS))
        .unwrap();
    assert!(line.contains("（当天）"), "相对时间: {line}");
    assert!(
        line.contains(&format_absolute_time(FIXED_TS)),
        "可读绝对时间不可省: {line}"
    );
    assert!(
        line.contains(&format!("ts={FIXED_TS}")),
        "Unix 秒级时间戳不可省: {line}"
    );
    assert!(
        line.contains(&format!("ts={FIXED_TS}（当天）")),
        "ts 与相对标签都挂在元信息里，且顺序稳定: {line}"
    );
    assert!(line.contains("[m_8f3a2b1]"), "反查把手: {line}");
    assert!(
        !memory.to_prompt_summary().unwrap().contains("（当天）"),
        "无锚点场景（5 条参考块）不标注相对时间"
    );
    for tier in MemoryIndexTier::ALL {
        let line = memory
            .to_index_prompt_at(tier, tier.segment_chars(), Some(FIXED_TS))
            .unwrap();
        assert!(line.contains("（当天）"), "{tier:?} 元信息不分级: {line}");
    }
}

/// 空窗口 / 单条 / 无锚点兜底都不能 panic 或丢行
#[test]
fn plan_handles_empty_and_falls_back_without_anchor() {
    assert!(
        MemoryIndexPlan::build(&[], Some(FIXED_TS), 2_000, char_cost)
            .tiers
            .is_empty()
    );
    let one = vec![short_term_days_ago(0, "唯一")];
    assert_eq!(
        MemoryIndexPlan::build(&one, Some(FIXED_TS), 2_000, char_cost).tier_at(0),
        Some(MemoryIndexTier::Full)
    );
    let fallback = MemoryIndexPlan::fallback(&one, 2_000, char_cost);
    assert_eq!(fallback.anchor_ts, None, "兜底计划没有 T0");
    assert_eq!(fallback.tier_at(0), Some(MemoryIndexTier::Full));
}

/// 元信息不分级：四档共用同一前缀，只有摘要正文变短
/// （护栏：禁止改回「远处只留 [id]」的分级形态）
#[test]
fn index_metadata_is_uniform_and_only_body_decays() {
    let memory = short_term(&segmented_text());
    let lines: Vec<String> = [
        MemoryIndexTier::Full,
        MemoryIndexTier::ThreeParts,
        MemoryIndexTier::TwoParts,
        MemoryIndexTier::OnePart,
    ]
    .iter()
    .map(|&tier| memory.to_index_prompt(tier, tier.segment_chars()).unwrap())
    .collect();

    let full_line = memory.to_prompt_summary().unwrap();
    let meta_end = full_line.find(" 甲").expect("元信息后接摘要");
    let meta = &full_line[..meta_end];
    assert!(
        lines.iter().all(|line| line.starts_with(meta)),
        "四档元信息必须逐字节一致: {lines:?}"
    );

    let lengths: Vec<usize> = lines.iter().map(|line| line.chars().count()).collect();
    assert!(
        lengths.windows(2).all(|w| w[0] >= w[1]),
        "只有正文可以递减: {lengths:?}"
    );
    assert!(lengths[0] > lengths[3], "远档确实更短: {lengths:?}");
}

/// 全文档安全阀：超长摘要不能整条灌进索引块（可用 query_memory 反查全文）
#[test]
fn full_tier_caps_overlong_summaries() {
    let long = short_term(&"甲".repeat(MEMORY_INDEX_FULL_MAX_CHARS * 3));
    let line = long.to_index_prompt(MemoryIndexTier::Full, 64).unwrap();
    assert!(
        line.chars().count() <= MEMORY_INDEX_FULL_MAX_CHARS + 64,
        "全文行必须封顶: {} 字",
        line.chars().count()
    );
    assert!(line.contains('…'), "超长部分要有省略标记: {line}");
    assert!(
        long.to_prompt_summary().unwrap().chars().count() > MEMORY_INDEX_FULL_MAX_CHARS * 2,
        "to_prompt_summary 保持真·全文，不走安全阀"
    );
}

/// 衰减系数确实让远档每段更短，但有下限兜底（不能只剩几个字）
#[test]
fn segment_chars_decays_with_ratio_and_keeps_floor() {
    let chars: [usize; 4] = [
        MemoryIndexTier::Full,
        MemoryIndexTier::ThreeParts,
        MemoryIndexTier::TwoParts,
        MemoryIndexTier::OnePart,
    ]
    .map(|t| t.segment_chars());
    assert_eq!(chars[0], MEMORY_INDEX_SEGMENT_BASE_CHARS);
    assert!(
        chars.windows(2).all(|w| w[0] >= w[1]),
        "逐档递减: {chars:?}"
    );
    assert!(
        chars.iter().all(|&c| c >= MEMORY_INDEX_SEGMENT_MIN_CHARS),
        "下限保护: {chars:?}"
    );
}

/// 元信息里的 Unix 时间戳是稳定事实：四档一致，且毫秒/秒级输入统一折算成秒
#[test]
fn index_timestamp_is_uniform_and_unit_normalized() {
    let expected = format!("ts={FIXED_TS}");
    for tier in MemoryIndexTier::ALL {
        let line = short_term("内容")
            .to_index_prompt(tier, tier.segment_chars())
            .unwrap();
        assert!(
            line.contains(&expected),
            "{tier:?} 必须带秒级时间戳: {line}"
        );
    }
    let mut millis = short_term("内容");
    if let MemoryPo::ShortTerm(st) = &mut millis.po {
        st.created_at = FIXED_TS * 1000;
    }
    assert_eq!(
        millis.to_index_prompt(MemoryIndexTier::Full, 64),
        short_term("内容").to_index_prompt(MemoryIndexTier::Full, 64),
        "毫秒时间戳不能让 ts 或可读时间漂移（前缀缓存护栏）"
    );
}

/// 知识节点与短期记忆对称：同一对 prompt 方法，元信息不分级、只有正文衰减
#[test]
fn knowledge_node_has_symmetric_prompt_and_index_prompt() {
    let mut memory = node("摘要兜底");
    if let MemoryPo::KnowledgeNode(k) = &mut memory.po {
        k.node_description = "甲".repeat(200);
        k.user_tags = r#"["缓存","前缀"]"#.to_string();
    }
    let full = memory.to_prompt_summary().unwrap();
    assert!(
        full.starts_with("[知识节点 kn_1] 订单状态机 · general"),
        "查边把手恒在行首: {full}"
    );
    assert!(full.contains("#缓存 #前缀"), "标签进元信息: {full}");
    assert!(full.contains(&"甲".repeat(200)), "全文不截断: {full}");

    let mut lines = Vec::new();
    for tier in MemoryIndexTier::ALL {
        lines.push(memory.to_index_prompt(tier, tier.segment_chars()).unwrap());
    }
    let meta_end = full.find(" · 甲").expect("元信息后接正文");
    let meta = &full[..meta_end];
    assert!(
        lines.iter().all(|line| line.starts_with(meta)),
        "四档元信息必须逐字节一致: {lines:?}"
    );
    assert!(
        lines[3].contains('…') && lines[3].chars().count() < full.chars().count(),
        "远档只留片段: {lines:?}"
    );
}

/// 正文口径下沉在 PO：description 空回退 summary，两者都空则整行不出
#[test]
fn knowledge_node_body_fallback_is_in_the_po() {
    let mut memory = node("摘要兜底");
    if let MemoryPo::KnowledgeNode(k) = &mut memory.po {
        k.node_description.clear();
    }
    assert!(memory.to_prompt_summary().unwrap().contains("摘要兜底"));
    assert!(
        memory
            .to_index_prompt(MemoryIndexTier::OnePart, 8)
            .unwrap()
            .contains("摘要兜底")
    );

    if let MemoryPo::KnowledgeNode(k) = &mut memory.po {
        k.summary.clear();
    }
    assert!(memory.to_prompt_summary().is_none());
    assert!(
        memory
            .to_index_prompt(MemoryIndexTier::OnePart, 8)
            .is_none()
    );
}

/// 统一入口分发两类实体；知识节点没有相对时间，T0 锚点对它无意义
#[test]
fn unified_prompt_entry_dispatches_both_entity_types() {
    assert!(short_term("内容").to_prompt_summary().is_some());
    let knowledge = node("摘要");
    assert!(knowledge.to_prompt_summary().is_some());
    assert_eq!(
        knowledge.to_index_prompt_at(MemoryIndexTier::OnePart, 8, Some(FIXED_TS)),
        knowledge.to_index_prompt(MemoryIndexTier::OnePart, 8),
        "知识节点元信息不含相对时间，锚点不能让它漂移"
    );
}

/// 按需翻页信号：预算装满 = 停；候选集吃光仍未装满 = 再查一页
#[test]
fn plan_saturation_drives_paging() {
    let memories = bulk_memories(60);
    let small = MemoryIndexPlan::build(&memories, Some(FIXED_TS), 500, char_cost);
    assert!(small.saturated, "小预算应装满并提前收尾");
    assert!(!small.needs_more(), "装满后不许再翻页");

    let large = MemoryIndexPlan::build(&memories, Some(FIXED_TS), 20_000, char_cost);
    assert_eq!(large.tiers.iter().flatten().count(), 60);
    assert!(!large.saturated, "预算远大于候选集 → 还要更早的记忆");
    assert!(large.needs_more());
}

/// 翻页单调性：多查一页更早的记忆，不能改写已加载（较新）那批的档位
#[test]
fn paging_does_not_rewrite_tiers_of_already_loaded_rows() {
    let memories = bulk_memories(60);
    let first_page = MemoryIndexPlan::build(&memories[40..], Some(FIXED_TS), 5_000, char_cost);
    let two_pages = MemoryIndexPlan::build(&memories[20..], Some(FIXED_TS), 5_000, char_cost);

    assert!(first_page.needs_more(), "第一页还喂不饱预算");
    assert_eq!(
        &two_pages.tiers[20..],
        first_page.tiers.as_slice(),
        "补进更早的行后，原有 20 条的档位必须逐字节不变"
    );
}
