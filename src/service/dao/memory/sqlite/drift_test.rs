//! Memory DAO 本体漂移（drift）单元测试：词频聚合、明细下钻与发布置位

use crate::models::memory::{KnowledgeNodeRelationPo, LongTermKnowledgeNodePo};
use crate::service::dao::memory::TermFrequencyRow;
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use common::api::PaginationParams;
use common::enums::{KnowledgeRelationStatus, MemoryStatus};
use sqlx::SqlitePool;

// ==================== 本体词频聚合（P1-6：漂移看板 SQL 之家） ====================

/// 词频聚合 + 明细下钻（关系侧）：GROUP BY 只计生效边，Superseded 历史边不污染看板
#[sqlx::test]
async fn test_word_freq_relations_and_detail(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    // 3 个节点
    for (i, name) in ["甲", "乙", "丙"].iter().enumerate() {
        dao.save_knowledge_node(
            ctx.clone(),
            &LongTermKnowledgeNodePo {
                id: format!("wf-n{}", i + 1),
                agent_id: "wf-agent".to_string(),
                node_name: name.to_string(),
                node_description: String::new(),
                node_type: "concept".to_string(),
                summary: String::new(),
                tags: "[]".to_string(),
                status: MemoryStatus::Active,
                is_published: false,
                created_at: 100 + i as i64,
                updated_at: 100 + i as i64,
            },
        )
        .await
        .unwrap();
    }

    // 3 条生效边（related ×2、contains ×1）+ 1 条被替换的历史边（related）
    let mk_relation =
        |id: &str, src: &str, dst: &str, ty: &str, at: i64, st: KnowledgeRelationStatus| {
            KnowledgeNodeRelationPo {
                id: id.to_string(),
                source_node_id: src.to_string(),
                target_node_id: dst.to_string(),
                relation_type: ty.to_string(),
                weight: None,
                status: st,
                created_at: at,
                updated_at: at,
            }
        };
    dao.add_knowledge_relation(
        ctx.clone(),
        &mk_relation(
            "wf-r1",
            "wf-n1",
            "wf-n2",
            "related",
            201,
            KnowledgeRelationStatus::Active,
        ),
    )
    .await
    .unwrap();
    dao.add_knowledge_relation(
        ctx.clone(),
        &mk_relation(
            "wf-r2",
            "wf-n2",
            "wf-n3",
            "related",
            202,
            KnowledgeRelationStatus::Active,
        ),
    )
    .await
    .unwrap();
    dao.add_knowledge_relation(
        ctx.clone(),
        &mk_relation(
            "wf-r3",
            "wf-n1",
            "wf-n3",
            "contains",
            203,
            KnowledgeRelationStatus::Active,
        ),
    )
    .await
    .unwrap();
    dao.add_knowledge_relation(
        ctx.clone(),
        &mk_relation(
            "wf-r-sup",
            "wf-n3",
            "wf-n1",
            "related",
            204,
            KnowledgeRelationStatus::Superseded,
        ),
    )
    .await
    .unwrap();

    // 词频：只有生效边计入，Superseded 不算；单 Agent 场景 agent_count 恒为 1
    let freq = dao.word_freq_relations(ctx.clone(), None).await.unwrap();
    assert_eq!(
        freq,
        vec![
            TermFrequencyRow {
                term: "related".to_string(),
                count: 2,
                agent_count: 1
            },
            TermFrequencyRow {
                term: "contains".to_string(),
                count: 1,
                agent_count: 1
            },
        ]
    );

    // 明细下钻：按归一化词匹配（None = 全局视图），只回生效边，最近创建优先
    let detail = dao
        .detail_relations_by_term(
            ctx.clone(),
            "related",
            None,
            PaginationParams {
                limit: Some(10),
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(detail.items.len(), 2, "Superseded 边不得出现在明细中");
    assert_eq!(detail.total, 2, "total 与过滤后全量一致");
    assert_eq!(detail.items[0].id, "wf-r2", "最近创建优先");
    assert_eq!(detail.items[1].id, "wf-r1");
    // 瘦投影：agent_id 取源节点归属，名称由 JOIN 两端节点取得
    assert_eq!(detail.items[0].agent_id, "wf-agent");
    assert_eq!(detail.items[0].source_name, "乙");
    assert_eq!(detail.items[0].target_name, "丙");
    assert_eq!(detail.items[1].source_name, "甲");
    assert_eq!(detail.items[1].target_name, "乙");

    // limit 截断：items 只回 1 条，total 仍报全量
    let truncated = dao
        .detail_relations_by_term(
            ctx.clone(),
            "related",
            None,
            PaginationParams {
                limit: Some(1),
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(truncated.items.len(), 1);
    assert_eq!(truncated.items[0].id, "wf-r2");
    assert_eq!(truncated.total, 2);

    // offset 跳页：跳过最近的 wf-r2，total 不变
    let skipped = dao
        .detail_relations_by_term(
            ctx.clone(),
            "related",
            None,
            PaginationParams {
                limit: Some(1),
                offset: Some(1),
            },
        )
        .await
        .unwrap();
    assert_eq!(skipped.items.len(), 1);
    assert_eq!(
        skipped.items[0].id, "wf-r1",
        "offset=1 跳过最近创建的 wf-r2"
    );
    assert_eq!(skipped.total, 2);

    // 不存在的词：空列表，不报错
    let missing = dao
        .detail_relations_by_term(
            ctx,
            "no-such-term",
            None,
            PaginationParams {
                limit: Some(10),
                offset: None,
            },
        )
        .await
        .unwrap();
    assert!(missing.items.is_empty());
    assert_eq!(missing.total, 0);
}

/// 词频聚合 + 明细下钻（节点侧）：排除已遗忘；词表外开放类型值必须可查可数
#[sqlx::test]
async fn test_word_freq_nodes_and_detail(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let mk_node = |id: &str, ty: &str, st: MemoryStatus, at: i64| LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: "wf-agent".to_string(),
        node_name: id.to_string(),
        node_description: String::new(),
        node_type: ty.to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        status: st,
        is_published: false,
        created_at: at,
        updated_at: at,
    };

    // concept ×2 + event ×1 + Forgotten concept ×1 + 词表外开放类型 ×1
    dao.save_knowledge_node(
        ctx.clone(),
        &mk_node("wf-c1", "concept", MemoryStatus::Active, 301),
    )
    .await
    .unwrap();
    dao.save_knowledge_node(
        ctx.clone(),
        &mk_node("wf-c2", "concept", MemoryStatus::Active, 302),
    )
    .await
    .unwrap();
    dao.save_knowledge_node(
        ctx.clone(),
        &mk_node("wf-e1", "event", MemoryStatus::Active, 303),
    )
    .await
    .unwrap();
    dao.save_knowledge_node(
        ctx.clone(),
        &mk_node("wf-f1", "concept", MemoryStatus::Forgotten, 304),
    )
    .await
    .unwrap();
    dao.save_knowledge_node(
        ctx.clone(),
        &mk_node("wf-x1", "drift-xyz", MemoryStatus::Active, 305),
    )
    .await
    .unwrap();

    // 词频：Forgotten 不计；词表外的 drift-xyz 与内置类型同台计数；单 Agent agent_count 恒为 1
    let freq = dao.word_freq_nodes(ctx.clone(), None).await.unwrap();
    assert_eq!(
        freq,
        vec![
            TermFrequencyRow {
                term: "concept".to_string(),
                count: 2,
                agent_count: 1
            },
            TermFrequencyRow {
                term: "drift-xyz".to_string(),
                count: 1,
                agent_count: 1
            },
            TermFrequencyRow {
                term: "event".to_string(),
                count: 1,
                agent_count: 1
            },
        ]
    );

    // 明细下钻：排除 Forgotten
    let concept_detail = dao
        .detail_nodes_by_term(
            ctx.clone(),
            "concept",
            None,
            PaginationParams {
                limit: Some(10),
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        concept_detail.items.len(),
        2,
        "Forgotten 节点不得出现在明细中"
    );
    assert_eq!(concept_detail.total, 2, "total 与过滤后全量一致");
    assert_eq!(concept_detail.items[0].id, "wf-c2", "最近更新优先");
    assert_eq!(concept_detail.items[1].id, "wf-c1");

    // 关键语义：词表外的开放类型值必须能下钻（不走 MemoryType 校验），
    // 否则看板只能看到漂移词名却查不到任何明细
    let drift_detail = dao
        .detail_nodes_by_term(
            ctx.clone(),
            "drift-xyz",
            None,
            PaginationParams {
                limit: Some(10),
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(drift_detail.items.len(), 1);
    assert_eq!(drift_detail.total, 1);
    assert_eq!(drift_detail.items[0].id, "wf-x1");
    assert_eq!(drift_detail.items[0].name, "wf-x1");
    assert_eq!(drift_detail.items[0].agent_id, "wf-agent");
}

// ==================== P2-1c：多 Agent 词频 + agent 过滤 + publish 置位 ====================

/// 节点侧多 Agent 聚合：agent_count 去重 + 归一化合并（大小写/空白变体并为一词）
#[sqlx::test]
async fn test_word_freq_nodes_multi_agent_and_filter(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let mk_node = |id: &str, agent: &str, ty: &str, at: i64| LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: agent.to_string(),
        node_name: id.to_string(),
        node_description: String::new(),
        node_type: ty.to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: at,
        updated_at: at,
    };

    // a1: concept ×2（其中一个是 "Concept " 大小写+空白变体）；a2: concept ×1、event ×1
    dao.save_knowledge_node(ctx.clone(), &mk_node("wc-a1", "agent-a1", "concept", 401))
        .await
        .unwrap();
    dao.save_knowledge_node(ctx.clone(), &mk_node("wc-a2", "agent-a1", "Concept ", 402))
        .await
        .unwrap();
    dao.save_knowledge_node(ctx.clone(), &mk_node("wc-a3", "agent-a2", "concept", 403))
        .await
        .unwrap();
    dao.save_knowledge_node(ctx.clone(), &mk_node("wc-a4", "agent-a2", "event", 404))
        .await
        .unwrap();

    // 归一化聚合：concept 3 条 / 2 个 agent；event 1 条 / 1 个 agent
    let freq = dao.word_freq_nodes(ctx.clone(), None).await.unwrap();
    assert_eq!(
        freq,
        vec![
            TermFrequencyRow {
                term: "concept".to_string(),
                count: 3,
                agent_count: 2
            },
            TermFrequencyRow {
                term: "event".to_string(),
                count: 1,
                agent_count: 1
            },
        ]
    );

    // agent 过滤：只统计该 Agent 的节点（agent_count 恒为 1）
    let filtered = dao
        .word_freq_nodes(ctx.clone(), Some("agent-a1".to_string()))
        .await
        .unwrap();
    assert_eq!(
        filtered,
        vec![TermFrequencyRow {
            term: "concept".to_string(),
            count: 2,
            agent_count: 1
        }],
        "agent-a1 只有 concept ×2，event 属于 agent-a2"
    );

    // 空串过滤 = 全局（与 None 同义）
    let global = dao
        .word_freq_nodes(ctx.clone(), Some(String::new()))
        .await
        .unwrap();
    assert_eq!(global, freq, "空串过滤等价于不过滤");

    // 归一化下钻：词表命中的归一化词能查出所有大小写/空白变体
    let normalized = dao
        .detail_nodes_by_term(
            ctx.clone(),
            "concept",
            Some("agent-a1".to_string()),
            PaginationParams {
                limit: None,
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        normalized.total, 2,
        "agent-a1 的 concept 与 Concept 变体都命中"
    );
    assert!(normalized.items.iter().all(|r| r.agent_id == "agent-a1"));

    // 空串 = 不过滤（与 None 同义，全局视图）
    let global = dao
        .detail_nodes_by_term(
            ctx.clone(),
            "concept",
            Some(String::new()),
            PaginationParams {
                limit: None,
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        global.total, 3,
        "空串过滤 = 全局，3 个 agent 的 concept 全命中"
    );
}

/// 关系侧 agent 归属：关系表无 agent 列，按 LEFT JOIN 源节点归属去重与过滤
#[sqlx::test]
async fn test_word_freq_relations_agent_count(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let mk_node = |id: &str, agent: &str, at: i64| LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: agent.to_string(),
        node_name: id.to_string(),
        node_description: String::new(),
        node_type: "concept".to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: at,
        updated_at: at,
    };
    let mk_relation = |id: &str, src: &str, dst: &str, ty: &str, at: i64| KnowledgeNodeRelationPo {
        id: id.to_string(),
        source_node_id: src.to_string(),
        target_node_id: dst.to_string(),
        relation_type: ty.to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: at,
        updated_at: at,
    };

    dao.save_knowledge_node(ctx.clone(), &mk_node("wm-m1", "agent-a1", 501))
        .await
        .unwrap();
    dao.save_knowledge_node(ctx.clone(), &mk_node("wm-m2", "agent-a2", 502))
        .await
        .unwrap();

    // related 边的源节点分属两个 agent → agent_count = 2；depends 边源节点是 a1
    dao.add_knowledge_relation(
        ctx.clone(),
        &mk_relation("wm-r1", "wm-m1", "wm-m2", "related", 511),
    )
    .await
    .unwrap();
    dao.add_knowledge_relation(
        ctx.clone(),
        &mk_relation("wm-r2", "wm-m2", "wm-m1", "related", 512),
    )
    .await
    .unwrap();
    dao.add_knowledge_relation(
        ctx.clone(),
        &mk_relation("wm-r3", "wm-m1", "wm-m1", "depends", 513),
    )
    .await
    .unwrap();

    let freq = dao.word_freq_relations(ctx.clone(), None).await.unwrap();
    assert_eq!(
        freq,
        vec![
            TermFrequencyRow {
                term: "related".to_string(),
                count: 2,
                agent_count: 2
            },
            TermFrequencyRow {
                term: "depends".to_string(),
                count: 1,
                agent_count: 1
            },
        ]
    );

    // agent 过滤（源节点归属）：a1 视角只剩源为 a1 的 related 边与 depends 边，count 并列按 term 字典序
    let a1_freq = dao
        .word_freq_relations(ctx.clone(), Some("agent-a1".to_string()))
        .await
        .unwrap();
    assert_eq!(
        a1_freq,
        vec![
            TermFrequencyRow {
                term: "depends".to_string(),
                count: 1,
                agent_count: 1
            },
            TermFrequencyRow {
                term: "related".to_string(),
                count: 1,
                agent_count: 1
            },
        ],
        "agent-a1 视角：wm-r2（源为 a2）被过滤，related 只剩 1 条"
    );

    // 空串过滤 = 全局（与 None 同义）
    let global = dao
        .word_freq_relations(ctx.clone(), Some(String::new()))
        .await
        .unwrap();
    assert_eq!(global, freq, "空串过滤等价于不过滤");

    // agent 过滤按源节点归属：a1 只看到源为 a1 的 related 边
    let a1_view = dao
        .detail_relations_by_term(
            ctx.clone(),
            "related",
            Some("agent-a1".to_string()),
            PaginationParams {
                limit: None,
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(a1_view.total, 1, "agent 过滤按源节点归属");
    assert_eq!(a1_view.items[0].id, "wm-r1");
    assert_eq!(a1_view.items[0].agent_id, "agent-a1");
    assert_eq!(a1_view.items[0].source_name, "wm-m1");
    assert_eq!(a1_view.items[0].target_name, "wm-m2");
}

/// publish_nodes_by_type：归一化匹配 + 幂等置位 + Forgotten 排除
#[sqlx::test]
async fn test_publish_nodes_by_type(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let mk_node = |id: &str, ty: &str, st: MemoryStatus, at: i64| LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: "wp-agent".to_string(),
        node_name: id.to_string(),
        node_description: String::new(),
        node_type: ty.to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        status: st,
        is_published: false,
        created_at: at,
        updated_at: at,
    };

    // 存活 concept ×2（一个带大小写/空白变体）+ 存活 event ×1 + Forgotten concept ×1
    dao.save_knowledge_node(
        ctx.clone(),
        &mk_node("wp-c1", "concept", MemoryStatus::Active, 601),
    )
    .await
    .unwrap();
    dao.save_knowledge_node(
        ctx.clone(),
        &mk_node("wp-c2", "Concept ", MemoryStatus::Active, 602),
    )
    .await
    .unwrap();
    dao.save_knowledge_node(
        ctx.clone(),
        &mk_node("wp-e1", "event", MemoryStatus::Active, 603),
    )
    .await
    .unwrap();
    dao.save_knowledge_node(
        ctx.clone(),
        &mk_node("wp-f1", "concept", MemoryStatus::Forgotten, 604),
    )
    .await
    .unwrap();

    // 首次置位：归一化命中的存活行 = 2（变体 "Concept " 一并置位，Forgotten 排除）
    let first = dao
        .publish_nodes_by_type(ctx.clone(), "concept")
        .await
        .unwrap();
    assert_eq!(
        first, 2,
        "归一化命中 2 个存活行（含大小写/空白变体），Forgotten 不置位"
    );

    // 幂等：已置位的行不重复计数
    let again = dao
        .publish_nodes_by_type(ctx.clone(), "concept")
        .await
        .unwrap();
    assert_eq!(again, 0, "幂等重复调用返回 0");

    // 其他类型不受影响
    let other = dao
        .publish_nodes_by_type(ctx.clone(), "event")
        .await
        .unwrap();
    assert_eq!(other, 1);

    // 置位落库效果直接验证（瘦投影不带 is_published）
    let published_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM long_term_knowledge_node WHERE LOWER(TRIM(node_type)) = 'concept' AND is_published = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(published_count.0, 2);
    let event_published: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM long_term_knowledge_node WHERE node_type = 'event' AND is_published = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(event_published.0, 1);
}
