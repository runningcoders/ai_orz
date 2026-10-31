//! 属性有效性 DAO 单测：取值「一把尺子」的 SQL 序列化分支（真库覆盖）
//!
//! 纯函数层（`crate::service::dal::property_effectiveness_test`）只验分组 / 众数 /
//! 阈值判定；**取值序列化发生在 SQL 的 `CASE` 分支里**，只有真库能覆盖。
//!
//! 现网 1227 条生效值的载体分布是 text 1083 / ref 136 / bool 8、**num 0 条**，
//! 所以 `value_num` 的整数化分支此前零覆盖——本文件补这一格。它的风险不是「跑不到」，
//! 而是「哪天跑到就悄悄错」：`1.0` 若原样落成 `"1.0"`，同一个 term_key 下
//! 「1」与「1.0」会被拆成两组，众数占比被稀释，看板从此漏报近似常量。
//!
//! 节点 / 属性全部走 DAO 固有方法（`save_knowledge_node` / `upsert_property`），
//! 不经 DAL 软门禁编排——本文件验的是聚合口径，不是写入校验。

use crate::models::memory::{KnowledgePropertyPo, LongTermKnowledgeNodePo};
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use common::enums::MemoryStatus;
use common::enums::ontology::PropertyTargetType;
use common::ontology_property::PropertyValue;
use sqlx::SqlitePool;

/// 建一个 `concept` 节点作为属性值载体（`node_type` 决定聚合分组）
async fn seed_node(dao: &MemoryDaoSqliteImpl, ctx: &crate::pkg::RequestContext, id: &str) {
    dao.save_knowledge_node(
        ctx.clone(),
        &LongTermKnowledgeNodePo {
            id: id.to_string(),
            agent_id: "num-agent".to_string(),
            node_name: id.to_string(),
            node_description: String::new(),
            node_type: "concept".to_string(),
            summary: String::new(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: 100,
            updated_at: 100,
        },
    )
    .await
    .unwrap();
}

/// `value_num` 整数化去尾零：`1.0` → `"1"`、`0.9` → `"0.9"`，且两者不合并
///
/// 这是现网零覆盖的分支（无 num 行）：一旦整数化失效，`1.0` 会落成 `"1.0"`，
/// 与 uint 分支/文本载体写下的 `"1"` 拆成两组，`distinct_values` 虚高、
/// `mode_ratio` 被稀释 → 该标黄的组合不再标黄。
#[sqlx::test]
async fn test_value_num_serializes_without_trailing_zeros(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("num-user", pool.clone());

    seed_node(&dao, &ctx, "kn_num_a").await;
    seed_node(&dao, &ctx, "kn_num_b").await;

    // 同一 term_key 的两条 number 值：整数 1.0 与小数 0.9
    for (target, value) in [("kn_num_a", 1.0_f64), ("kn_num_b", 0.9_f64)] {
        dao.upsert_property(
            ctx.clone(),
            &KnowledgePropertyPo::new(
                PropertyTargetType::Node,
                target,
                "confidence",
                PropertyValue::Number(value),
                0,
            ),
        )
        .await
        .unwrap();
    }

    let rows = dao
        .property_value_freq(ctx.clone(), Some("confidence".to_string()), None)
        .await
        .unwrap();

    assert_eq!(
        rows.len(),
        2,
        "1.0 与 0.9 必须序列化成两个不同取值行，不能合并成一组"
    );
    let mut serialized: Vec<&str> = rows.iter().map(|r| r.value_text.as_str()).collect();
    serialized.sort_unstable();
    assert_eq!(
        serialized,
        vec!["0.9", "1"],
        "整数化去尾零口径：1.0 → \"1\"、0.9 → \"0.9\"（原样 CAST 会给出 \"1.0\"）"
    );
    assert!(
        rows.iter().all(|r| r.count == 1),
        "每个取值各 1 条，实际 {:?}",
        rows.iter().map(|r| r.count).collect::<Vec<_>>()
    );
    assert!(
        rows.iter()
            .all(|r| r.term_key == "confidence" && r.node_type == "concept"),
        "分组键必须回填 term_key / node_type"
    );
}

/// 「一把尺子」：不同值列序列化后同形 ⇒ 合并成同一取值组
///
/// number `1.0` 与 string `"1"` 写法不同、语义同形，序列化后都是 `"1"`。
/// 若各列分别分组，同一个属性会因为「值落在哪个列」被拆成多组——这正是
/// 子查询先统一序列化再分组的目的。
#[sqlx::test]
async fn test_different_value_columns_share_group_when_serialization_matches(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("num-user", pool.clone());

    seed_node(&dao, &ctx, "kn_mix_a").await;
    seed_node(&dao, &ctx, "kn_mix_b").await;

    for (target, value) in [
        ("kn_mix_a", PropertyValue::Number(1.0)),
        ("kn_mix_b", PropertyValue::Text("1".to_string())),
    ] {
        dao.upsert_property(
            ctx.clone(),
            &KnowledgePropertyPo::new(PropertyTargetType::Node, target, "confidence", value, 0),
        )
        .await
        .unwrap();
    }

    let rows = dao
        .property_value_freq(ctx.clone(), Some("confidence".to_string()), None)
        .await
        .unwrap();

    assert_eq!(
        rows.len(),
        1,
        "number 1.0 与 string \"1\" 序列化同形，必须合并成同一取值组（一把尺子）"
    );
    assert_eq!(rows[0].value_text, "1");
    assert_eq!(rows[0].count, 2, "两条载体同值 ⇒ 计数 2");
}
