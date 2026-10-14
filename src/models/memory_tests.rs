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
        kn.tags = r#"["published","架构"]"#.to_string();
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
