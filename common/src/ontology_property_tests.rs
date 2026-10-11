//! ontology_property 单元测试

use super::*;

// ==================== data_type 映射 ====================

#[test]
fn value_maps_to_declared_data_type() {
    assert_eq!(
        PropertyValue::Text("a".into()).data_type(),
        PropertyDataType::String
    );
    assert_eq!(
        PropertyValue::Number(1.0).data_type(),
        PropertyDataType::Number
    );
    assert_eq!(
        PropertyValue::Boolean(true).data_type(),
        PropertyDataType::Boolean
    );
    assert_eq!(
        PropertyValue::Date("d".into()).data_type(),
        PropertyDataType::Date
    );
    assert_eq!(
        PropertyValue::Reference("n1".into()).data_type(),
        PropertyDataType::Reference
    );
    assert_eq!(
        PropertyValue::Enum("e".into()).data_type(),
        PropertyDataType::Enum
    );
}

// ==================== 值 ↔ 列互转 ====================

#[test]
fn into_columns_writes_only_the_assigned_column() {
    let cols = PropertyValue::Text("hello".into()).into_columns();
    assert_eq!(cols.value_text.as_deref(), Some("hello"));
    assert!(cols.value_num.is_none() && cols.value_bool.is_none() && cols.value_ref.is_none());

    let cols = PropertyValue::Number(0.75).into_columns();
    assert_eq!(cols.value_num, Some(0.75));
    assert!(cols.value_text.is_none());

    let cols = PropertyValue::Boolean(true).into_columns();
    assert_eq!(cols.value_bool, Some(1));

    let cols = PropertyValue::Reference("node-1".into()).into_columns();
    assert_eq!(cols.value_ref.as_deref(), Some("node-1"));
    assert!(cols.value_text.is_none());
}

#[test]
fn boolean_false_maps_to_zero_not_null() {
    // 落库必须区分 false 与「未赋值」：false 是 0，未赋值是 NULL
    assert_eq!(
        PropertyValue::Boolean(false).into_columns().value_bool,
        Some(0)
    );
}

#[test]
fn from_columns_roundtrip_all_types() {
    let cases = [
        PropertyValue::Text("t".into()),
        PropertyValue::Number(2.5),
        PropertyValue::Boolean(true),
        PropertyValue::Date("2026-10-11".into()),
        PropertyValue::Reference("node-9".into()),
        PropertyValue::Enum("open".into()),
    ];
    for value in cases {
        let data_type = value.data_type();
        let restored = PropertyValue::from_columns(data_type, value.clone().into_columns());
        assert_eq!(restored, Some(value), "往返失败: {data_type:?}");
    }
}

#[test]
fn from_columns_missing_assigned_column_returns_none() {
    // 声明 number 但数值列为 NULL = 脏数据，不猜 0 不 panic
    assert_eq!(
        PropertyValue::from_columns(PropertyDataType::Number, PropertyColumns::default()),
        None
    );
    assert_eq!(
        PropertyValue::from_columns(PropertyDataType::String, PropertyColumns::default()),
        None
    );
}

#[test]
fn from_columns_ignores_foreign_columns() {
    // 历史行改类型后残留旧列值，不得被误读
    let cols = PropertyColumns {
        value_text: Some("stale".into()),
        value_num: Some(9.0),
        ..Default::default()
    };
    assert_eq!(
        PropertyValue::from_columns(PropertyDataType::Number, cols),
        Some(PropertyValue::Number(9.0))
    );
}

// ==================== 取值约束解析 ====================

#[test]
fn parse_value_range_empty_string_is_no_constraint() {
    assert!(parse_value_range("").is_empty());
    assert!(parse_value_range("   ").is_empty());
}

#[test]
fn parse_value_range_array_is_enum_whitelist() {
    let range = parse_value_range(r#"["gpt-4", "claude"]"#);
    assert_eq!(
        range.enum_values,
        vec!["gpt-4".to_string(), "claude".to_string()]
    );
    assert!(!range.is_empty());
}

#[test]
fn parse_value_range_object_reads_min_max() {
    let range = parse_value_range(r#"{"min": 0.0, "max": 1.0}"#);
    assert_eq!(range.min, Some(0.0));
    assert_eq!(range.max, Some(1.0));
    assert!(range.enum_values.is_empty());
}

#[test]
fn parse_value_range_object_may_carry_enum_key() {
    let range = parse_value_range(r#"{"enum": ["a", "b"]}"#);
    assert_eq!(range.enum_values, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn parse_value_range_malformed_degrades_to_no_constraint() {
    // 软门禁：脏定义不失效整条写值链路
    assert!(parse_value_range("not-json").is_empty());
    assert!(parse_value_range("42").is_empty());
    assert!(parse_value_range("null").is_empty());
}

// ==================== 值校验 ====================

#[test]
fn validate_accepts_matching_type_without_range() {
    let range = ValueRange::default();
    assert_eq!(
        validate_property_value(
            PropertyDataType::String,
            &PropertyValue::Text("x".into()),
            &range
        ),
        Ok(())
    );
}

#[test]
fn validate_rejects_type_mismatch() {
    let range = ValueRange::default();
    assert_eq!(
        validate_property_value(
            PropertyDataType::Number,
            &PropertyValue::Text("x".into()),
            &range
        ),
        Err(PropertyValueIssue::TypeMismatch {
            expected: PropertyDataType::Number
        })
    );
}

#[test]
fn validate_number_within_range_passes() {
    let range = parse_value_range(r#"{"min": 0.0, "max": 1.0}"#);
    assert_eq!(
        validate_property_value(
            PropertyDataType::Number,
            &PropertyValue::Number(0.5),
            &range
        ),
        Ok(())
    );
    // 闭区间：边界值自身合法
    assert_eq!(
        validate_property_value(
            PropertyDataType::Number,
            &PropertyValue::Number(1.0),
            &range
        ),
        Ok(())
    );
}

#[test]
fn validate_number_out_of_range_rejected() {
    let range = parse_value_range(r#"{"min": 0.0, "max": 1.0}"#);
    assert_eq!(
        validate_property_value(
            PropertyDataType::Number,
            &PropertyValue::Number(1.5),
            &range
        ),
        Err(PropertyValueIssue::NumberOutOfRange)
    );
    assert_eq!(
        validate_property_value(
            PropertyDataType::Number,
            &PropertyValue::Number(-0.1),
            &range
        ),
        Err(PropertyValueIssue::NumberOutOfRange)
    );
}

#[test]
fn validate_number_rejects_non_finite() {
    // NaN/inf 与任何区间比较都为 false，必须显式拦下，否则静默写穿约束
    let range = parse_value_range(r#"{"min": 0.0, "max": 1.0}"#);
    assert_eq!(
        validate_property_value(
            PropertyDataType::Number,
            &PropertyValue::Number(f64::NAN),
            &range
        ),
        Err(PropertyValueIssue::NumberOutOfRange)
    );
    assert_eq!(
        validate_property_value(
            PropertyDataType::Number,
            &PropertyValue::Number(f64::INFINITY),
            &range
        ),
        Err(PropertyValueIssue::NumberOutOfRange)
    );
}

#[test]
fn validate_enum_whitelist() {
    let range = parse_value_range(r#"["open", "closed"]"#);
    assert_eq!(
        validate_property_value(
            PropertyDataType::Enum,
            &PropertyValue::Enum("open".into()),
            &range
        ),
        Ok(())
    );
    assert_eq!(
        validate_property_value(
            PropertyDataType::Enum,
            &PropertyValue::Enum("paused".into()),
            &range
        ),
        Err(PropertyValueIssue::NotInEnumValue)
    );
}

#[test]
fn validate_enum_without_whitelist_accepts_any() {
    // 白名单为空 = 管理员未约束，不拦截
    let range = ValueRange::default();
    assert_eq!(
        validate_property_value(
            PropertyDataType::Enum,
            &PropertyValue::Enum("any".into()),
            &range
        ),
        Ok(())
    );
}

// ==================== 基数判定 ====================

#[test]
fn allows_append_respects_cardinality() {
    assert!(allows_append(Cardinality::Single, 0));
    assert!(!allows_append(Cardinality::Single, 1));
    assert!(allows_append(Cardinality::Many, 0));
    assert!(allows_append(Cardinality::Many, 5));
}

// ==================== 枚举解析兜底 ====================

#[test]
fn data_type_and_cardinality_parse_or_default() {
    assert_eq!(
        PropertyDataType::parse_or_default("number"),
        PropertyDataType::Number
    );
    assert_eq!(
        PropertyDataType::parse_or_default("bogus"),
        PropertyDataType::String
    );
    assert_eq!(Cardinality::parse_or_default("many"), Cardinality::Many);
    assert_eq!(Cardinality::parse_or_default("bogus"), Cardinality::Single);
}

// ==================== 适用载体门闩 ====================

#[test]
fn applies_to_empty_means_unconstrained() {
    // 空清单 = 未声明 = 不限载体（与 domain_classes / value_range 的退化口径一致）
    assert!(allows_target(&[], PropertyTargetType::Node));
    assert!(allows_target(&[], PropertyTargetType::Relation));
}

#[test]
fn applies_to_non_empty_gates_by_exact_match() {
    let node_only = vec![PropertyTargetType::Node];
    assert!(allows_target(&node_only, PropertyTargetType::Node));
    assert!(
        !allows_target(&node_only, PropertyTargetType::Relation),
        "只声明 node 的属性不得挂到边上"
    );

    let both = vec![PropertyTargetType::Node, PropertyTargetType::Relation];
    assert!(allows_target(&both, PropertyTargetType::Relation));
}

#[test]
fn applies_to_parse_handles_dirty_json() {
    assert_eq!(parse_applies_to(""), Vec::new());
    assert_eq!(parse_applies_to("not json"), Vec::new());
    assert_eq!(parse_applies_to("{}"), Vec::new());
    assert_eq!(
        parse_applies_to(r#"["node"]"#),
        vec![PropertyTargetType::Node]
    );
    assert_eq!(
        parse_applies_to(r#"["node","relation"]"#),
        vec![PropertyTargetType::Node, PropertyTargetType::Relation]
    );
    // 拼错载体名静默丢弃 → 结果是谁都不让挂（窄化比放宽安全）
    assert_eq!(parse_applies_to(r#"["nodes"]"#), Vec::new());
    // 数组里的非字符串元素同样丢弃
    assert_eq!(
        parse_applies_to(r#"["node", 42]"#),
        vec![PropertyTargetType::Node]
    );
}

#[test]
fn target_type_as_str_matches_ddl_check_values() {
    assert_eq!(PropertyTargetType::Node.as_str(), "node");
    assert_eq!(PropertyTargetType::Relation.as_str(), "relation");
    assert_eq!(
        PropertyTargetType::parse_or_default("bogus"),
        PropertyTargetType::Node
    );
}

// ==================== 属性反查条件 ====================

#[test]
fn filter_op_is_supported_matches_value_type() {
    let numeric = |op| PropertyFilter {
        term_key: "score".into(),
        target_type: PropertyTargetType::Node,
        op,
        value: PropertyValue::Number(0.5),
    };
    // 数值支持全部比较
    for op in [
        PropertyFilterOp::Eq,
        PropertyFilterOp::Gt,
        PropertyFilterOp::Gte,
        PropertyFilterOp::Lt,
        PropertyFilterOp::Lte,
    ] {
        assert!(numeric(op).is_supported(), "{op:?} 应支持数值");
    }

    // 非数值只支持 Eq
    let text = |op| PropertyFilter {
        term_key: "model".into(),
        target_type: PropertyTargetType::Node,
        op,
        value: PropertyValue::Text("gpt-4".into()),
    };
    assert!(text(PropertyFilterOp::Eq).is_supported());
    assert!(!text(PropertyFilterOp::Gt).is_supported());

    let flag = PropertyFilter {
        term_key: "enabled".into(),
        target_type: PropertyTargetType::Node,
        op: PropertyFilterOp::Gte,
        value: PropertyValue::Boolean(true),
    };
    assert!(!flag.is_supported(), "布尔没有可比较的序");
}

#[test]
fn filter_carries_target_type_and_value_variant() {
    // 值变体决定 DAO 查哪一列，调用方不感知列映射
    let f = PropertyFilter {
        term_key: "effective_from".into(),
        target_type: PropertyTargetType::Relation,
        op: PropertyFilterOp::Eq,
        value: PropertyValue::Date("2026-10-11".into()),
    };
    assert_eq!(f.target_type, PropertyTargetType::Relation);
    assert_eq!(f.value.data_type(), PropertyDataType::Date);
}

#[test]
fn data_type_as_str_matches_ddl_check_values() {
    // 落库值必须与迁移 CHECK 枚举逐字一致
    assert_eq!(PropertyDataType::String.as_str(), "string");
    assert_eq!(PropertyDataType::Number.as_str(), "number");
    assert_eq!(PropertyDataType::Boolean.as_str(), "boolean");
    assert_eq!(PropertyDataType::Date.as_str(), "date");
    assert_eq!(PropertyDataType::Reference.as_str(), "reference");
    assert_eq!(PropertyDataType::Enum.as_str(), "enum");
    assert_eq!(Cardinality::Single.as_str(), "single");
    assert_eq!(Cardinality::Many.as_str(), "many");
}

// ==================== 写入门闩判定 ====================

#[test]
fn only_type_mismatch_and_cardinality_reject_write() {
    // 软门禁：值域 / 载体 / 未定义三类只记录、照常写入；
    // 只有「写错」（类型不符）与「无处安置」（基数超限）拒写
    assert!(
        PropertyWriteIssue::TypeMismatch {
            expected: PropertyDataType::Number
        }
        .rejects_write()
    );
    assert!(PropertyWriteIssue::CardinalityExceeded.rejects_write());
    assert!(!PropertyWriteIssue::NumberOutOfRange.rejects_write());
    assert!(!PropertyWriteIssue::NotInEnumValue.rejects_write());
    assert!(!PropertyWriteIssue::TargetNotAllowed.rejects_write());
    assert!(!PropertyWriteIssue::UndefinedProperty.rejects_write());
}

#[test]
fn every_issue_has_a_human_readable_reason() {
    // 报告直接进日志与看板，文案缺失会渲染出空白行
    let kinds = [
        PropertyWriteIssue::TypeMismatch {
            expected: PropertyDataType::String,
        },
        PropertyWriteIssue::NumberOutOfRange,
        PropertyWriteIssue::NotInEnumValue,
        PropertyWriteIssue::CardinalityExceeded,
        PropertyWriteIssue::TargetNotAllowed,
        PropertyWriteIssue::UndefinedProperty,
    ];
    for k in kinds {
        assert!(!k.describe().is_empty(), "{k:?} 缺少文案");
    }
}

#[test]
fn save_report_splits_rejected_from_warnings() {
    let report = PropertySaveReport {
        accepted: 2,
        issues: vec![
            PropertyIssue {
                term_key: "a".into(),
                kind: PropertyWriteIssue::TypeMismatch {
                    expected: PropertyDataType::Number,
                },
            },
            PropertyIssue {
                term_key: "b".into(),
                kind: PropertyWriteIssue::NumberOutOfRange,
            },
        ],
    };
    assert!(!report.is_clean());
    assert_eq!(report.rejected().count(), 1);
    assert_eq!(report.warnings().count(), 1);
    assert_eq!(report.rejected().next().unwrap().term_key, "a");
    assert_eq!(report.warnings().next().unwrap().term_key, "b");
}

// ==================== 取值提示（提示词注入） ====================

#[test]
fn value_range_hint_renders_numeric_bounds() {
    assert_eq!(parse_value_range(r#"{"min":0,"max":1}"#).to_hint(), "0~1");
    assert_eq!(parse_value_range(r#"{"min":0}"#).to_hint(), ">=0");
    assert_eq!(parse_value_range(r#"{"max":100}"#).to_hint(), "<=100");
    // 整数不带小数点：`1` 而非 `1.0`
    assert_eq!(
        parse_value_range(r#"{"min":0,"max":1.5}"#).to_hint(),
        "0~1.5"
    );
}

#[test]
fn value_range_hint_renders_enum_whitelist() {
    assert_eq!(
        parse_value_range(r#"["gpt-4","claude"]"#).to_hint(),
        "取值取其一: gpt-4|claude"
    );
}

/// 超长枚举截断：提示串进 prompt（受词表预算约束），不能吃掉整段预算
#[test]
fn value_range_hint_truncates_long_enum() {
    let items: Vec<String> = (0..20).map(|i| format!("v{}", i)).collect();
    let raw = serde_json::to_string(&items).expect("serialize");
    let hint = parse_value_range(&raw).to_hint();
    assert!(hint.starts_with("取值取其一: v0|v1|"));
    assert!(hint.ends_with("|…(共20项)"));
    assert!(hint.len() < 80, "截断后的提示串应保持短小：{hint}");
}

/// 区间与白名单并存（对象形态带 enum）→ 两段都渲染
#[test]
fn value_range_hint_renders_both_bounds_and_enum() {
    let hint = parse_value_range(r#"{"min":0,"max":10,"enum":["a","b"]}"#).to_hint();
    assert_eq!(hint, "0~10，取值取其一: a|b");
}

/// 无约束 → 空串（提示词里不渲染多余的方括号内容）
#[test]
fn value_range_hint_empty_when_unconstrained() {
    assert_eq!(ValueRange::default().to_hint(), "");
    assert_eq!(parse_value_range("").to_hint(), "");
    assert_eq!(parse_value_range("not json").to_hint(), "");
}

#[test]
fn empty_save_report_is_clean() {
    // 「全部通过」必须可判定，否则调用方要自己数 issues
    let report = PropertySaveReport::default();
    assert!(report.is_clean());
    assert_eq!(report.accepted, 0);
}
