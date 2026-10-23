//! Memory DAL 实体属性（ABox）测试：写入四道门闩（类型 / 载体 / 值域 / 基数）、
//! 未定义术语兜底、版本化保留历史、批量回读只出生效值、按属性精确召回。

use super::dal;
use super::test_support::*;
use common::enums::ontology::{PropertyDataType, PropertyTargetType};
use common::ontology_property::{
    PropertyEntry, PropertyFilter, PropertyFilterOp, PropertyValue, PropertyWriteIssue,
};

// ==================== 属性写入四道门闩（DB 级端到端） ====================
//
// 纯函数层（common/src/ontology_property_tests.rs）只验判定函数本身；这里的用例
// 走**真实库**：定义进 `ontology_properties`、值进 `knowledge_property`，断言
// 「判定」与「落库结果」一致——包括版本化后旧行仍在、反查通道取到正确 id。

/// 门闩 2（类型错）**硬拦**：值不落库，报告里是一条 rejected
#[sqlx::test]
async fn save_entity_properties_rejects_type_mismatch(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;
    define_property(&ctx, &property_po("confidence", "number", "", "single", "")).await;

    let report = dal()
        .save_entity_properties(
            ctx.clone(),
            PropertyTargetType::Node,
            "kn_a",
            vec![PropertyEntry {
                term_key: "confidence".to_string(),
                value: PropertyValue::Text("很高".to_string()),
                ordinal: 0,
            }],
        )
        .await
        .expect("save should return report (not Err)");

    assert_eq!(report.accepted, 0, "类型错应整条拒写");
    assert_eq!(report.rejected().count(), 1);
    assert!(matches!(
        report.issues[0].kind,
        PropertyWriteIssue::TypeMismatch { expected } if expected == PropertyDataType::Number
    ));
    // 硬拦 = 库里一行都没有（不是「写进去再打标」）
    assert_eq!(count_property_rows(&ctx, "kn_a").await, 0);
}

/// 门闩 3（值域越界）是**软门禁**：照常落库，只在报告里留告警
#[sqlx::test]
async fn save_entity_properties_writes_out_of_range_with_warning(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;
    define_property(
        &ctx,
        &property_po("confidence", "number", r#"{"min":0,"max":1}"#, "single", ""),
    )
    .await;

    let report = dal()
        .save_entity_properties(
            ctx.clone(),
            PropertyTargetType::Node,
            "kn_a",
            vec![PropertyEntry {
                term_key: "confidence".to_string(),
                value: PropertyValue::Number(5.0),
                ordinal: 0,
            }],
        )
        .await
        .expect("save should succeed");

    assert_eq!(report.accepted, 1, "值域越界不拒写（认知分歧不由写侧裁决）");
    assert_eq!(report.warnings().count(), 1);
    assert_eq!(report.rejected().count(), 0);
    assert!(matches!(
        report.issues[0].kind,
        PropertyWriteIssue::NumberOutOfRange
    ));
    // 值真的在库里，越界不影响可读
    let rows = dal()
        .list_entity_properties(ctx, PropertyTargetType::Node, "kn_a")
        .await
        .expect("list should succeed");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value(), Some(PropertyValue::Number(5.0)));
}

/// 门闩 1（载体不符）软门禁：记告警 + 照写（本体包容，写侧不裁决）
#[sqlx::test]
async fn save_entity_properties_warns_on_disallowed_target(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;
    // applies_to 只允许挂边
    define_property(
        &ctx,
        &property_po("since", "date", "", "single", r#"["relation"]"#),
    )
    .await;

    let report = dal()
        .save_entity_properties(
            ctx.clone(),
            PropertyTargetType::Node,
            "kn_a",
            vec![PropertyEntry {
                term_key: "since".to_string(),
                value: PropertyValue::Date("2026-10-11".to_string()),
                ordinal: 0,
            }],
        )
        .await
        .expect("save should succeed");

    assert_eq!(report.accepted, 1);
    assert!(matches!(
        report.issues[0].kind,
        PropertyWriteIssue::TargetNotAllowed
    ));
    assert_eq!(report.rejected().count(), 0, "载体不符是软门禁，不拒写");
}

/// 门闩 0（本体未定义）兜底写入：按值自身类型落库，不丢数据
#[sqlx::test]
async fn save_entity_properties_falls_back_for_undefined_term(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;

    let report = dal()
        .save_entity_properties(
            ctx.clone(),
            PropertyTargetType::Node,
            "kn_a",
            vec![PropertyEntry {
                term_key: "ad_hoc_note".to_string(),
                value: PropertyValue::Text("临时观察".to_string()),
                ordinal: 0,
            }],
        )
        .await
        .expect("save should succeed");

    assert_eq!(report.accepted, 1, "未定义属性也写（术语漂移是常态）");
    assert!(matches!(
        report.issues[0].kind,
        PropertyWriteIssue::UndefinedProperty
    ));
    let rows = dal()
        .list_entity_properties(ctx, PropertyTargetType::Node, "kn_a")
        .await
        .expect("list should succeed");
    assert_eq!(
        rows[0].value(),
        Some(PropertyValue::Text("临时观察".to_string()))
    );
}

/// 版本化：单值属性重复写 = 旧行软取代 + 插新行，读侧只见新值、历史仍留
#[sqlx::test]
async fn save_entity_properties_supersedes_single_value_and_keeps_history(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;
    define_property(&ctx, &property_po("confidence", "number", "", "single", "")).await;
    // ⚠️ dal() 返回临时 Arc：async trait 方法的 future 借用 &self，
    // 闭包里现取现用会撞 E0515（返回引用临时值），先绑定到局部变量再调两次
    let d = dal();
    for v in [0.3, 0.8] {
        d.save_entity_properties(
            ctx.clone(),
            PropertyTargetType::Node,
            "kn_a",
            vec![PropertyEntry {
                term_key: "confidence".to_string(),
                value: PropertyValue::Number(v),
                ordinal: 0,
            }],
        )
        .await
        .expect("write should succeed");
    }

    // 读侧：一条生效值 = 最新值（旧行被软取代，不是被覆盖也不是被删除）
    let rows = dal()
        .list_entity_properties(ctx.clone(), PropertyTargetType::Node, "kn_a")
        .await
        .expect("list should succeed");
    assert_eq!(rows.len(), 1, "读侧只见生效行");
    assert_eq!(rows[0].value(), Some(PropertyValue::Number(0.8)));
    // 物理行：两次写入都在（行级软取代保留全历史）
    assert_eq!(count_property_rows(&ctx, "kn_a").await, 2);
}

/// 批量回读只出 status=1 生效值（修复「回读带出 status=0 已取代旧值」缺陷的防回归守卫）
///
/// 复刻实证场景：reference 型多值属性 related_task_id 软取代后，批量回读
/// （search_memory attach_properties 消费链）不得再带出旧值。
#[sqlx::test]
async fn list_entity_properties_batch_returns_only_active_rows(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;
    define_property(
        &ctx,
        &property_po("related_task_id", "reference", "", "many", ""),
    )
    .await;
    let d = dal();

    for task_id in [
        "01a0de98-0000-7000-8000-000000000001",
        "01a0de98-0000-7000-8000-000000000002",
    ] {
        d.save_entity_properties(
            ctx.clone(),
            PropertyTargetType::Node,
            "kn_batch",
            vec![PropertyEntry {
                term_key: "related_task_id".to_string(),
                value: PropertyValue::Reference(task_id.to_string()),
                ordinal: 0,
            }],
        )
        .await
        .expect("write should succeed");
    }

    // 物理行两条（历史保留），活跃层只允许最新一条
    assert_eq!(count_property_rows(&ctx, "kn_batch").await, 2);
    let map = d
        .list_entity_properties_batch(
            ctx.clone(),
            &[(PropertyTargetType::Node, "kn_batch".to_string())],
        )
        .await
        .expect("batch read should succeed");
    let rows = map
        .get(&(
            PropertyTargetType::Node.as_str().to_string(),
            "kn_batch".to_string(),
        ))
        .expect("活跃值应存在");
    assert_eq!(rows.len(), 1, "批量回读只出 status=1 生效值: {rows:?}");
    assert_eq!(
        rows[0].value(),
        Some(PropertyValue::Reference(
            "01a0de98-0000-7000-8000-000000000002".to_string()
        ))
    );
}

/// 门闩 4（基数）硬拦：管理员把 many 收紧为 single 后，已占的其他槽位堵住新写
#[sqlx::test]
async fn save_entity_properties_rejects_cardinality_overflow(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;
    define_property(&ctx, &property_po("alias", "string", "", "many", "")).await;

    // many：两个槽位各一条
    let report = dal()
        .save_entity_properties(
            ctx.clone(),
            PropertyTargetType::Node,
            "kn_a",
            vec![
                PropertyEntry {
                    term_key: "alias".to_string(),
                    value: PropertyValue::Text("A".to_string()),
                    ordinal: 0,
                },
                PropertyEntry {
                    term_key: "alias".to_string(),
                    value: PropertyValue::Text("B".to_string()),
                    ordinal: 1,
                },
            ],
        )
        .await
        .expect("save should succeed");
    assert_eq!(report.accepted, 2, "many 属性可占多槽位");

    // 管理员收紧基数：many → single
    let dao = crate::service::dao::ontology::dao();
    let mut def = dao
        .find_property_by_term_key(ctx.clone(), "alias")
        .await
        .expect("find should succeed")
        .expect("alias should be defined");
    def.cardinality = "single".to_string();
    dao.update_property(ctx.clone(), &def)
        .await
        .expect("update should succeed");

    // single 强制槽位 0，但槽位 1 已被占 ⇒ 无处安置，拒写
    let report = dal()
        .save_entity_properties(
            ctx.clone(),
            PropertyTargetType::Node,
            "kn_a",
            vec![PropertyEntry {
                term_key: "alias".to_string(),
                value: PropertyValue::Text("C".to_string()),
                ordinal: 0,
            }],
        )
        .await
        .expect("save should return report");
    assert_eq!(report.accepted, 0);
    assert!(matches!(
        report.issues[0].kind,
        PropertyWriteIssue::CardinalityExceeded
    ));
    // 存量两条不受影响（拒写不回滚既有数据）
    let rows = dal()
        .list_entity_properties(ctx, PropertyTargetType::Node, "kn_a")
        .await
        .expect("list should succeed");
    assert_eq!(rows.len(), 2);
}

/// 属性反查通道：数值范围条件精确召回载体 id（不是「在语义 top_k 里再筛」）
#[sqlx::test]
async fn query_entities_by_properties_recalls_by_numeric_range(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;
    define_property(
        &ctx,
        &property_po("confidence", "number", r#"{"min":0,"max":1}"#, "single", ""),
    )
    .await;
    for (id, v) in [("kn_a", 0.9), ("kn_b", 0.2)] {
        dal()
            .save_entity_properties(
                ctx.clone(),
                PropertyTargetType::Node,
                id,
                vec![PropertyEntry {
                    term_key: "confidence".to_string(),
                    value: PropertyValue::Number(v),
                    ordinal: 0,
                }],
            )
            .await
            .expect("save should succeed");
    }

    let mut ids = dal()
        .query_entities_by_properties(
            ctx.clone(),
            &[PropertyFilter {
                term_key: "confidence".to_string(),
                target_type: PropertyTargetType::Node,
                op: PropertyFilterOp::Gte,
                value: PropertyValue::Number(0.5),
            }],
        )
        .await
        .expect("query should succeed");
    ids.sort();
    assert_eq!(ids, vec!["kn_a".to_string()], "只有高置信节点命中");
}
