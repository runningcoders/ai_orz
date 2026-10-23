//! Memory DAO 知识节点关系（relations）单元测试：建边、批量、端点校验与原文往返

use crate::models::memory::{KnowledgeNodeRelationPo, LongTermKnowledgeNodePo};
use crate::service::dao::memory::MemoryQuery;
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use common::enums::{KnowledgeRelationStatus, MemoryStatus};
use sqlx::SqlitePool;

#[sqlx::test]
async fn test_add_knowledge_relation(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    // 先创建两个节点
    let node1 = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "Rust".to_string(),
        node_description: "Rust 编程语言".to_string(),
        node_type: "language".to_string(),
        summary: "Rust 是一门系统编程语言".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    };
    let node2 = LongTermKnowledgeNodePo {
        id: "node-2".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "内存安全".to_string(),
        node_description: "内存安全特性".to_string(),
        node_type: "concept".to_string(),
        summary: "内存安全是 Rust 的核心特性".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    };

    dao.save_knowledge_node(ctx.clone(), &node1).await.unwrap();
    dao.save_knowledge_node(ctx.clone(), &node2).await.unwrap();

    // 添加关系（带强度）
    let relation = KnowledgeNodeRelationPo {
        id: "rel-1".to_string(),
        source_node_id: "node-1".to_string(),
        target_node_id: "node-2".to_string(),
        relation_type: "related".to_string(),
        weight: Some(0.75),
        status: KnowledgeRelationStatus::Active,
        created_at: 0,
        updated_at: 0,
    };

    let result = dao.add_knowledge_relation(ctx.clone(), &relation).await;
    assert!(result.is_ok());

    // 查询验证
    let relations = dao
        .list_outgoing_relations(ctx.clone(), "node-1")
        .await
        .unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].source_node_id, "node-1");
    assert_eq!(relations[0].target_node_id, "node-2");
    // 强度必须落库并原样读回：漏列的表现是「图上线宽全都一样」，不报错
    assert_eq!(relations[0].weight, Some(0.75));
}

#[sqlx::test]
async fn test_knowledge_relations(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    // 创建节点
    let nodes = vec![
        LongTermKnowledgeNodePo {
            id: "rel-1".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "节点1".to_string(),
            node_description: "节点1描述".to_string(),
            node_type: "concept".to_string(),
            summary: "节点1摘要".to_string(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: now,
            updated_at: now,
        },
        LongTermKnowledgeNodePo {
            id: "rel-2".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "节点2".to_string(),
            node_description: "节点2描述".to_string(),
            node_type: "concept".to_string(),
            summary: "节点2摘要".to_string(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: now,
            updated_at: now,
        },
    ];
    dao.batch_save_knowledge_nodes(ctx.clone(), &nodes)
        .await
        .unwrap();

    // 测试 batch_add_knowledge_relations
    let relations = vec![KnowledgeNodeRelationPo {
        id: "rel-rel-1-2".to_string(),
        source_node_id: "rel-1".to_string(),
        target_node_id: "rel-2".to_string(),
        relation_type: "related".to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: now,
        updated_at: now,
    }];
    dao.batch_add_knowledge_relations(ctx.clone(), &relations)
        .await
        .unwrap();

    // 测试 list_outgoing_relations
    let outgoing = dao
        .list_outgoing_relations(ctx.clone(), "rel-1")
        .await
        .unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].target_node_id, "rel-2");

    // 测试 list_incoming_relations
    let incoming = dao
        .list_incoming_relations(ctx.clone(), "rel-2")
        .await
        .unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].source_node_id, "rel-1");

    // 测试同键替换：新边插入时旧 Active 边降级为 Superseded（版本链）
    let superseding = vec![KnowledgeNodeRelationPo {
        id: "rel-rel-1-2-hist".to_string(),
        source_node_id: "rel-1".to_string(),
        target_node_id: "rel-2".to_string(),
        relation_type: "related".to_string(),
        weight: Some(0.9),
        status: KnowledgeRelationStatus::Active,
        created_at: now,
        updated_at: now,
    }];
    dao.batch_add_knowledge_relations(ctx.clone(), &superseding)
        .await
        .unwrap();
    let outgoing = dao
        .list_outgoing_relations(ctx.clone(), "rel-1")
        .await
        .unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].id, "rel-rel-1-2-hist");
    let old_status: i32 = sqlx::query_scalar(
        r#"SELECT "status" FROM knowledge_node_relation WHERE id = 'rel-rel-1-2'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(old_status, KnowledgeRelationStatus::Superseded as i32);

    // 测试 delete_knowledge_relation 只降级生效边：Superseded 历史边不可被覆盖成 Deleted，
    // 且 rows_affected 检查后必须**显式报错**（修复假成功：旧实现静默 Ok）
    let del_err = dao
        .delete_knowledge_relation(ctx.clone(), "rel-rel-1-2")
        .await
        .expect_err("删除非生效边必须报错（rows_affected 检查）");
    assert!(
        del_err.to_string().contains("resource_not_found"),
        "错误码应为 resource_not_found: {del_err}"
    );
    let old_status: i32 = sqlx::query_scalar(
        r#"SELECT "status" FROM knowledge_node_relation WHERE id = 'rel-rel-1-2'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(old_status, KnowledgeRelationStatus::Superseded as i32);

    // 测试 query_knowledge_relations（按 ids 精确取生效边，默认过滤非 Active）
    let found = dao
        .query_knowledge_relations(
            ctx.clone(),
            MemoryQuery {
                ids: Some(vec![
                    "rel-rel-1-2".to_string(),
                    "rel-rel-1-2-hist".to_string(),
                ]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "rel-rel-1-2-hist");

    // 无 ids 时返回空：关系是依附节点的派生视图，不支持全量扫描
    let empty = dao
        .query_knowledge_relations(ctx.clone(), MemoryQuery::default())
        .await
        .unwrap();
    assert!(empty.is_empty());

    // 测试 delete_knowledge_relation 软删除：标记 Deleted(2)，行保留支持恢复
    dao.delete_knowledge_relation(ctx.clone(), "rel-rel-1-2-hist")
        .await
        .unwrap();
    let outgoing_after = dao
        .list_outgoing_relations(ctx.clone(), "rel-1")
        .await
        .unwrap();
    assert_eq!(outgoing_after.len(), 0);
    let hist_status: i32 = sqlx::query_scalar(
        r#"SELECT "status" FROM knowledge_node_relation WHERE id = 'rel-rel-1-2-hist'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(hist_status, KnowledgeRelationStatus::Deleted as i32);

    // rows_affected 检查：删除不存在的边必须显式报错（同上，不再静默 Ok）
    let missing_err = dao
        .delete_knowledge_relation(ctx.clone(), "rel-not-exist")
        .await
        .expect_err("删除不存在的边必须报错");
    assert!(
        missing_err.to_string().contains("resource_not_found"),
        "错误码应为 resource_not_found: {missing_err}"
    );
}

/// 建边端点事务内防线：端点不存在/非活跃时报错且整体回滚，不落任何行
///
/// 复现 handler 批量预检 → 逐条落库之间端点被并发删除的场景：DAO 校验与
/// INSERT 同事务，任一端点缺失（含软删为 Forgotten）即报 SourceNodeNotFound /
/// TargetNodeNotFound，同批已插入的合法边也一并回滚。
#[sqlx::test]
async fn test_batch_add_knowledge_relations_rejects_inactive_endpoint(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    // 准备三个活跃节点 a / b / c
    for (id, name) in [
        ("edge-node-a", "节点A"),
        ("edge-node-b", "节点B"),
        ("edge-node-c", "节点C"),
    ] {
        dao.save_knowledge_node(
            ctx.clone(),
            &LongTermKnowledgeNodePo {
                id: id.to_string(),
                agent_id: "test-agent".to_string(),
                node_name: name.to_string(),
                node_description: format!("{name}描述"),
                node_type: "concept".to_string(),
                summary: format!("{name}摘要"),
                tags: "[]".to_string(),
                status: MemoryStatus::Active,
                is_published: false,
                created_at: now,
                updated_at: now,
            },
        )
        .await
        .unwrap();
    }

    let relation = |id: &str, src: &str, tgt: &str| KnowledgeNodeRelationPo {
        id: id.to_string(),
        source_node_id: src.to_string(),
        target_node_id: tgt.to_string(),
        relation_type: "related".to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: now,
        updated_at: now,
    };

    // 统计 relations 全表行数（含非生效行，验证「不落任何行」）
    let count_rows = || async {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_node_relation")
            .fetch_one(&pool)
            .await
            .unwrap();
        n
    };
    assert_eq!(count_rows().await, 0);

    // 1) 目标端点不存在 → TargetNodeNotFound，错误信息带 target_id，不落行
    let err = dao
        .batch_add_knowledge_relations(
            ctx.clone(),
            &[relation("kr-1", "edge-node-a", "edge-ghost")],
        )
        .await
        .expect_err("目标端点不存在必须报错");
    assert_eq!(err.code.code_str(), "target_node_not_found");
    assert!(
        err.msg.contains("edge-ghost"),
        "错误信息应带 target_id: {}",
        err.msg
    );
    assert_eq!(count_rows().await, 0, "报错后不得落任何行");

    // 2) 源端点不存在 → SourceNodeNotFound，错误信息带 source_id，不落行
    let err = dao
        .batch_add_knowledge_relations(
            ctx.clone(),
            &[relation("kr-2", "edge-ghost", "edge-node-b")],
        )
        .await
        .expect_err("源端点不存在必须报错");
    assert_eq!(err.code.code_str(), "source_node_not_found");
    assert!(
        err.msg.contains("edge-ghost"),
        "错误信息应带 source_id: {}",
        err.msg
    );
    assert_eq!(count_rows().await, 0);

    // 3) 端点被并发软删（Active → Forgotten）→ 同样拒绝，对齐 delete_knowledge_node 场景
    dao.delete_knowledge_node(ctx.clone(), "edge-node-b")
        .await
        .unwrap();
    let err = dao
        .batch_add_knowledge_relations(
            ctx.clone(),
            &[relation("kr-3", "edge-node-a", "edge-node-b")],
        )
        .await
        .expect_err("端点已软删（非活跃）必须报错");
    assert_eq!(err.code.code_str(), "target_node_not_found");
    assert_eq!(count_rows().await, 0);

    // 4) 批内「合法 + 非法」混合 → 整体回滚：已插入的合法边也不得残留
    let mixed = vec![
        relation("kr-4", "edge-node-a", "edge-node-c"), // 两端活跃，合法
        relation("kr-5", "edge-node-a", "edge-ghost"),  // 目标缺失，非法
    ];
    let err = dao
        .batch_add_knowledge_relations(ctx.clone(), &mixed)
        .await
        .expect_err("批内含非法端点必须整体回滚");
    assert_eq!(err.code.code_str(), "target_node_not_found");
    assert_eq!(count_rows().await, 0, "合法边必须随事务一并回滚");

    // 5) 正常建边回归：两端活跃时仍成功落库、可读回
    dao.batch_add_knowledge_relations(
        ctx.clone(),
        &[relation("kr-6", "edge-node-a", "edge-node-c")],
    )
    .await
    .expect("两端活跃的建边应成功");
    assert_eq!(count_rows().await, 1);
    let outgoing = dao
        .list_outgoing_relations(ctx.clone(), "edge-node-a")
        .await
        .unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].id, "kr-6");
    assert_eq!(outgoing[0].target_node_id, "edge-node-c");
}

/// list_relations_batch 分块：600 个 ID（每 ID 2 绑定 = 1200 > SQLite 999 上限），
/// 分块后不应报错，跨块关系应命中，且跨块拼接后保持 created_at ASC
#[sqlx::test]
async fn test_list_relations_batch_chunking(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    // 600 个 ID：分块大小 400 → 2 块；两条关系的端点分别落在不同块
    let ids: Vec<String> = (0..600).map(|i| format!("chunk-node-{:03}", i)).collect();

    let now = chrono::Utc::now().timestamp();
    // 早创建：两端都在第一块（id[100] → id[200]）
    dao.add_knowledge_relation(
        ctx.clone(),
        &KnowledgeNodeRelationPo {
            id: "chunk-rel-early".to_string(),
            source_node_id: ids[100].clone(),
            target_node_id: ids[200].clone(),
            relation_type: "related".to_string(),
            weight: None,
            status: KnowledgeRelationStatus::Active,
            created_at: now,
            updated_at: now,
        },
    )
    .await
    .unwrap();
    // 晚创建：跨块（id[0] 第一块 → id[500] 第二块）
    dao.add_knowledge_relation(
        ctx.clone(),
        &KnowledgeNodeRelationPo {
            id: "chunk-rel-cross".to_string(),
            source_node_id: ids[0].clone(),
            target_node_id: ids[500].clone(),
            relation_type: "related".to_string(),
            weight: None,
            status: KnowledgeRelationStatus::Active,
            created_at: now + 10,
            updated_at: now + 10,
        },
    )
    .await
    .unwrap();

    let results = dao.list_relations_batch(ctx, &ids).await.unwrap();
    assert_eq!(results.len(), 2, "两条关系都应命中（含跨块）");
    // 跨块拼接后仍按 created_at ASC 有序
    assert_eq!(results[0].id, "chunk-rel-early");
    assert_eq!(results[1].id, "chunk-rel-cross");
}

/// 关系类型**原文**必须原样往返：词表外的标注不能被归一化成 `custom`。
///
/// 回归：写入路径曾做 `KnowledgeRelationType::from()`，词表外的值（如「实现」）
/// 会被塌成 `Custom` 落库 —— 图谱上只剩「自定义」，写入方明确的语义永久丢失。
#[sqlx::test]
async fn relation_type_round_trips_verbatim(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    let nodes = vec![
        LongTermKnowledgeNodePo {
            id: "rt-a".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "节点A".to_string(),
            node_description: "节点A描述".to_string(),
            node_type: "concept".to_string(),
            summary: "节点A摘要".to_string(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: now,
            updated_at: now,
        },
        LongTermKnowledgeNodePo {
            id: "rt-b".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "节点B".to_string(),
            node_description: "节点B描述".to_string(),
            node_type: "concept".to_string(),
            summary: "节点B摘要".to_string(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: now,
            updated_at: now,
        },
    ];
    dao.batch_save_knowledge_nodes(ctx.clone(), &nodes)
        .await
        .unwrap();

    // 词表外的标注：必须逐字落库
    dao.add_knowledge_relation(
        ctx.clone(),
        &KnowledgeNodeRelationPo {
            id: "rt-rel-custom-text".to_string(),
            source_node_id: "rt-a".to_string(),
            target_node_id: "rt-b".to_string(),
            relation_type: "实现".to_string(),
            weight: None,
            status: KnowledgeRelationStatus::Active,
            created_at: now,
            updated_at: now,
        },
    )
    .await
    .unwrap();

    let stored = dao
        .list_outgoing_relations(ctx.clone(), "rt-a")
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(
        stored[0].relation_type, "实现",
        "词表外的关系原文必须原样落库，不能被塌成 custom"
    );

    // 按原文过滤能命中（比的是原文，不是枚举归类）
    let by_raw = dao
        .find_relations_by_type(ctx.clone(), "rt-a", "实现")
        .await
        .unwrap();
    assert_eq!(by_raw.len(), 1, "按原文查询应命中");
    let by_guessed = dao
        .find_relations_by_type(ctx.clone(), "rt-a", "custom")
        .await
        .unwrap();
    assert!(by_guessed.is_empty(), "不能把它当成 custom 存下来");

    // 规范词同样原样保存（不做 Display 重写）
    dao.add_knowledge_relation(
        ctx.clone(),
        &KnowledgeNodeRelationPo {
            id: "rt-rel-depends".to_string(),
            source_node_id: "rt-a".to_string(),
            target_node_id: "rt-b".to_string(),
            relation_type: "depends".to_string(),
            weight: None,
            status: KnowledgeRelationStatus::Active,
            created_at: now,
            updated_at: now,
        },
    )
    .await
    .unwrap();

    let all = dao
        .list_outgoing_relations(ctx.clone(), "rt-a")
        .await
        .unwrap();
    let kinds: Vec<&str> = all.iter().map(|r| r.relation_type.as_str()).collect();
    assert!(kinds.contains(&"实现"));
    assert!(kinds.contains(&"depends"));
}
