//! tests 单元测试（拆分自 ontology.rs）
//!
//! 文件瘦身：原 906 行 → 531 行，测试体 376 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

/// 自包含测试词表：不与枚举词表耦合，专注判定逻辑本身
fn fixture_lexicon() -> OntologyLexicon {
    OntologyLexicon {
        class_keys: ["agent", "task", "document"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        relation_keys: ["contains", "depends", "similar"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        relation_directions: [
            ("contains", Direction::Directed),
            ("depends", Direction::Directed),
            ("similar", Direction::Undirected),
        ]
        .into_iter()
        .map(|(k, d)| (k.to_string(), d))
        .collect(),
        synonyms: [
            (
                ("包含".to_string(), TermKind::Relation),
                "contains".to_string(),
            ),
            (
                ("依赖".to_string(), TermKind::Relation),
                "depends".to_string(),
            ),
            (
                ("object".to_string(), TermKind::Class),
                "document".to_string(),
            ),
        ]
        .into_iter()
        .collect(),
    }
}

#[test]
fn canonical_hit_on_relation_vocabulary() {
    let lexicon = fixture_lexicon();
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "contains"),
        ResolvedTerm::Canonical {
            term_key: "contains".to_string(),
            direction: Direction::Directed,
        }
    );
}

#[test]
fn canonical_hit_ignores_case_and_surrounding_whitespace() {
    let lexicon = fixture_lexicon();
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "  Contains "),
        ResolvedTerm::Canonical {
            term_key: "contains".to_string(),
            direction: Direction::Directed,
        }
    );
}

#[test]
fn via_synonym_hit_maps_to_target() {
    let lexicon = fixture_lexicon();
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "包含"),
        ResolvedTerm::ViaSynonym {
            term_key: "contains".to_string(),
            raw_term: "包含".to_string(),
            direction: Direction::Directed,
        }
    );
}

#[test]
fn via_synonym_target_must_belong_to_requested_kind() {
    let lexicon = fixture_lexicon();
    // "object" 映射到实体类 document；按关系词解析时应判漂移
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "object"),
        ResolvedTerm::Drift {
            raw_term: "object".to_string(),
            direction: Direction::Undirected,
        }
    );
    // 同一词条按实体类解析则同义命中
    assert_eq!(
        resolve(&lexicon, TermKind::Class, "object"),
        ResolvedTerm::ViaSynonym {
            term_key: "document".to_string(),
            raw_term: "object".to_string(),
            direction: Direction::Undirected,
        }
    );
}

#[test]
fn out_of_vocabulary_is_drift() {
    let lexicon = fixture_lexicon();
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "买卖"),
        ResolvedTerm::Drift {
            raw_term: "买卖".to_string(),
            direction: Direction::Undirected,
        }
    );
}

#[test]
fn kind_selects_which_vocabulary_is_checked() {
    let lexicon = fixture_lexicon();
    // "agent" 是实体类；按关系词解析应判漂移
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "agent"),
        ResolvedTerm::Drift {
            raw_term: "agent".to_string(),
            direction: Direction::Undirected,
        }
    );
    assert_eq!(
        resolve(&lexicon, TermKind::Class, "agent"),
        ResolvedTerm::Canonical {
            term_key: "agent".to_string(),
            direction: Direction::Undirected,
        }
    );
}

#[test]
fn canonical_takes_priority_over_synonym_entry() {
    let lexicon = fixture_lexicon();
    // 原文本身是规范词，即使同义映射里也有同名条目也不应绕道
    let mut lexicon = lexicon;
    lexicon.synonyms.insert(
        ("contains".to_string(), TermKind::Relation),
        "similar".to_string(),
    );
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "contains"),
        ResolvedTerm::Canonical {
            term_key: "contains".to_string(),
            direction: Direction::Directed,
        }
    );
}

#[test]
fn empty_raw_term_is_drift_not_panic() {
    let lexicon = fixture_lexicon();
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "   "),
        ResolvedTerm::Drift {
            raw_term: String::new(),
            direction: Direction::Undirected,
        }
    );
}

#[test]
fn same_input_and_lexicon_give_same_verdict() {
    let lexicon = fixture_lexicon();
    let first = resolve(&lexicon, TermKind::Relation, "  DEPENDS ");
    let second = resolve(&lexicon, TermKind::Relation, "depends");
    assert_eq!(first, second);
}

#[test]
fn term_kind_roundtrips_through_str() {
    assert_eq!(TermKind::Class.as_str(), "class");
    assert_eq!(TermKind::Relation.as_str(), "relation");
    assert_eq!("class".parse::<TermKind>(), Ok(TermKind::Class));
    assert_eq!("relation".parse::<TermKind>(), Ok(TermKind::Relation));
    assert!("edge".parse::<TermKind>().is_err());
}

#[test]
fn empty_lexicon_reports_everything_as_drift() {
    let lexicon = OntologyLexicon::default();
    assert!(lexicon.is_empty());
    assert_eq!(
        resolve(&lexicon, TermKind::Class, "agent"),
        ResolvedTerm::Drift {
            raw_term: "agent".to_string(),
            direction: Direction::Undirected,
        }
    );
}

#[test]
fn normalize_is_shared_convention() {
    assert_eq!(normalize("  Contains "), "contains");
    assert_eq!(normalize("AGENT"), "agent");
    // 刻意不去下划线：contained_by 与 containedby 必须保持可区分
    assert_eq!(normalize("contained_by"), "contained_by");
    assert_ne!(normalize("contained_by"), normalize("containedby"));
}

#[test]
fn certify_report_aggregates_counts_and_keys() {
    let lexicon = fixture_lexicon();
    let mut report = OntologyCertifyReport::default();
    report.record(
        TermKind::Relation,
        resolve(&lexicon, TermKind::Relation, "contains"),
    );
    report.record(
        TermKind::Relation,
        resolve(&lexicon, TermKind::Relation, "包含"),
    );
    report.record(
        TermKind::Relation,
        resolve(&lexicon, TermKind::Relation, "买卖"),
    );
    report.record(
        TermKind::Class,
        resolve(&lexicon, TermKind::Class, "document"),
    );
    assert_eq!(report.canonical_count(), 2);
    assert_eq!(report.via_synonym_count(), 1);
    assert_eq!(report.drift_count(), 1);
    assert_eq!(report.total_count(), 4);
    assert_eq!(report.certified_keys(), ["contains", "document"]);
    assert_eq!(report.drift_terms(), ["买卖"]);
    assert!(!report.is_clean());
}

#[test]
fn certify_report_dedups_keys_and_keeps_order() {
    let lexicon = fixture_lexicon();
    let mut report = OntologyCertifyReport::default();
    report.record(
        TermKind::Relation,
        resolve(&lexicon, TermKind::Relation, "contains"),
    );
    // 同一规范词的书写变体（归一化后同形）不应产生重复 key
    report.record(
        TermKind::Relation,
        resolve(&lexicon, TermKind::Relation, "  Contains "),
    );
    report.record(
        TermKind::Relation,
        resolve(&lexicon, TermKind::Relation, "depends"),
    );
    assert_eq!(report.certified_keys(), ["contains", "depends"]);
    assert!(report.drift_terms().is_empty());
    assert!(report.is_clean());
}

#[test]
fn preset_lexicon_deserializes_with_defaults() {
    let json = r#"{
        "classes": [
            {"term_key": "agent", "display_name": "智能体", "description": "自主执行单元"}
        ],
        "relation_types": [
            {"term_key": "contains", "display_name": "包含", "description": "组成关系"}
        ],
        "synonym_mappings": [
            {"raw_term": "包含", "target_kind": "relation", "target_key": "contains"}
        ]
    }"#;
    let preset: PresetOntologyLexicon = serde_json::from_str(json).expect("deserialize");
    assert_eq!(preset.classes.len(), 1);
    assert!(preset.classes[0].required_fields.is_empty());
    assert_eq!(preset.relation_types.len(), 1);
    assert_eq!(preset.relation_types[0].weight_base, None);
    assert_eq!(preset.relation_types[0].inverse_key, None);
    assert_eq!(preset.synonym_mappings.len(), 1);
    assert_eq!(preset.synonym_mappings[0].target_kind, TermKind::Relation);
    assert_eq!(preset.total_count(), 3);
    assert!(!preset.is_empty());
}

#[test]
fn empty_preset_lexicon_roundtrips() {
    let preset = PresetOntologyLexicon::default();
    assert!(preset.is_empty());
    assert_eq!(preset.total_count(), 0);
    let json = serde_json::to_string(&preset).expect("serialize");
    let back: PresetOntologyLexicon = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, preset);
}

// ==================== direction（三期方案 a′）====================

#[test]
fn resolve_relation_carries_lexicon_direction() {
    let lexicon = fixture_lexicon();
    // Canonical：方向 = 词表登记值
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "contains"),
        ResolvedTerm::Canonical {
            term_key: "contains".to_string(),
            direction: Direction::Directed,
        }
    );
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "similar"),
        ResolvedTerm::Canonical {
            term_key: "similar".to_string(),
            direction: Direction::Undirected,
        }
    );
    // ViaSynonym：方向 = 映射目标词的登记值
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "包含"),
        ResolvedTerm::ViaSynonym {
            term_key: "contains".to_string(),
            raw_term: "包含".to_string(),
            direction: Direction::Directed,
        }
    );
}

#[test]
fn resolve_drift_defaults_to_undirected() {
    let lexicon = fixture_lexicon();
    // Drift：词表外自拟词兜底无向（软门禁哲学，渲染无线箭头）
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "买卖"),
        ResolvedTerm::Drift {
            raw_term: "买卖".to_string(),
            direction: Direction::Undirected,
        }
    );
    // 词表缺方向登记（relation_directions 无该 key）同样兜底无向
    let mut lexicon = lexicon;
    lexicon.relation_directions.remove("contains");
    assert_eq!(
        resolve(&lexicon, TermKind::Relation, "contains"),
        ResolvedTerm::Canonical {
            term_key: "contains".to_string(),
            direction: Direction::Undirected,
        }
    );
}

#[test]
fn direction_parses_and_serializes_lowercase() {
    assert_eq!(Direction::Directed.as_str(), "directed");
    assert_eq!(Direction::Undirected.as_str(), "undirected");
    assert_eq!(Direction::default(), Direction::Undirected);
    assert_eq!("directed".parse::<Direction>(), Ok(Direction::Directed));
    assert_eq!("undirected".parse::<Direction>(), Ok(Direction::Undirected));
    assert_eq!(Direction::parse_or_default("directed"), Direction::Directed);
    assert_eq!(
        Direction::parse_or_default("garbage"),
        Direction::Undirected
    );
    // 非法值解析失败（由调用侧统一兜底 Undirected），不猜测
    assert!("both".parse::<Direction>().is_err());
    assert_eq!(
        serde_json::to_string(&Direction::Directed).unwrap(),
        "\"directed\""
    );
    assert_eq!(
        serde_json::from_str::<Direction>("\"undirected\"").unwrap(),
        Direction::Undirected
    );
}

#[test]
fn preset_relation_type_direction_defaults_to_undirected() {
    // 旧快照无 direction 字段：serde default 兜底，双向兼容不破 seed
    let json = r#"{"term_key": "contains", "display_name": "包含", "description": "组成关系"}"#;
    let preset: PresetOntologyRelationType = serde_json::from_str(json).expect("deserialize");
    assert_eq!(preset.direction, "undirected");

    let mut preset = preset;
    preset.direction = "directed".to_string();
    let out = serde_json::to_string(&preset).expect("serialize");
    assert!(out.contains("\"direction\":\"directed\""));
}
