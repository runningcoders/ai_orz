//! Memory DAL 写入侧测试：create（轨迹 / 短期记忆 / 知识节点 / 关系）、
//! delete（软删除与不可删除类型）、update（短期记忆 / 知识节点 / 不可修改类型）。

use super::test_support::*;
use crate::models::memory::{
    KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, Memory, MemoryCreateParams, MemoryPo,
    MemoryTrace, ShortTermMemoryIndexPo,
};
use crate::service::dao::memory::MemoryQuery;
use common::enums::{KnowledgeRelationStatus, MemoryStatus};
use common::error::Result;
use sqlx::SqlitePool;

// ========== create 方法测试 ==========

#[sqlx::test]
async fn test_create_append_traces(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建测试用的 MemoryTrace
    let trace = MemoryTrace::new(
        "agent-001".to_string(),
        "log-001".to_string(),
        "user-001".to_string(),
        "org-001".to_string(),
        common::enums::MemoryRole::User,
        "这是一条测试记忆内容".to_string(),
        None,
    );

    let params = MemoryCreateParams::AppendTraces(vec![trace]);
    let results = dal.create(ctx, params).await?;

    assert!(!results.is_empty());
    // 验证返回的是 Trace 类型
    match &results[0].po {
        crate::models::memory::MemoryPo::Trace(t) => {
            assert_eq!(t.agent_id, "agent-001");
            assert_eq!(t.input, "这是一条测试记忆内容");
            assert!(t.position.is_some()); // 写入后应该有位置信息
        }
        _ => panic!("预期返回 Trace 类型"),
    }

    Ok(())
}

#[sqlx::test]
async fn test_create_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建短期记忆索引
    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-test-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "测试短期记忆摘要".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let params = MemoryCreateParams::CreateShortTerm(index);
    let results = dal.create(ctx.clone(), params).await?;

    assert_eq!(results.len(), 1);

    // 验证返回的是 ShortTerm 类型
    match &results[0].po {
        crate::models::memory::MemoryPo::ShortTerm(st) => {
            assert_eq!(st.id, "st-test-001");
            assert_eq!(st.agent_id, "agent-001");
            assert_eq!(st.summary, "测试短期记忆摘要");
        }
        _ => panic!("预期返回 ShortTerm 类型"),
    }

    // 验证可以通过 query 查到
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        ..Default::default()
    };
    let query_results = dal.query(ctx, query).await?;
    assert_eq!(query_results.len(), 1);

    Ok(())
}

#[sqlx::test]
async fn test_create_knowledge_node(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建知识节点
    let now = chrono::Utc::now().timestamp();
    let node = LongTermKnowledgeNodePo {
        id: "kn-test-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "测试知识节点".to_string(),
        node_description: "这是一个测试知识节点的描述".to_string(),
        node_type: "concept".to_string(),
        summary: "测试知识节点的总结".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };

    // 没有引用的情况
    let params = MemoryCreateParams::CreateKnowledgeNode {
        node,
        references: vec![],
    };
    let results = dal.create(ctx.clone(), params).await?;

    assert_eq!(results.len(), 1);

    // 验证返回的是 KnowledgeNode 类型
    match &results[0].po {
        crate::models::memory::MemoryPo::KnowledgeNode(kn) => {
            assert_eq!(kn.id, "kn-test-001");
            assert_eq!(kn.agent_id, "agent-001");
            assert_eq!(kn.node_name, "测试知识节点");
        }
        _ => panic!("预期返回 KnowledgeNode 类型"),
    }

    Ok(())
}

#[sqlx::test]
async fn test_create_relations(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建两个知识节点
    let now = chrono::Utc::now().timestamp();
    let node1 = LongTermKnowledgeNodePo {
        id: "kn-source-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "源节点".to_string(),
        node_description: "源节点描述".to_string(),
        node_type: "concept".to_string(),
        summary: "源节点总结".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };
    let node2 = LongTermKnowledgeNodePo {
        id: "kn-target-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "目标节点".to_string(),
        node_description: "目标节点描述".to_string(),
        node_type: "concept".to_string(),
        summary: "目标节点总结".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };

    // 先创建节点
    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: node1,
            references: vec![],
        },
    )
    .await?;
    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: node2,
            references: vec![],
        },
    )
    .await?;

    // 创建关系
    let relation = KnowledgeNodeRelationPo {
        id: "rel-test-001".to_string(),
        source_node_id: "kn-source-001".to_string(),
        target_node_id: "kn-target-001".to_string(),
        relation_type: "related".to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let params = MemoryCreateParams::CreateRelations(vec![relation]);
    let results = dal.create(ctx, params).await?;

    assert_eq!(results.len(), 1);

    // 验证返回的是 Relation 类型
    match &results[0].po {
        crate::models::memory::MemoryPo::Relation(r) => {
            assert_eq!(r.id, "rel-test-001");
            assert_eq!(r.source_node_id, "kn-source-001");
            assert_eq!(r.target_node_id, "kn-target-001");
        }
        _ => panic!("预期返回 Relation 类型"),
    }

    Ok(())
}

// ========== delete 方法测试 ==========

#[sqlx::test]
async fn test_delete_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建短期记忆
    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-delete-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "待删除的短期记忆".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let params = MemoryCreateParams::CreateShortTerm(index.clone());
    dal.create(ctx.clone(), params).await?;

    // 验证存在
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        ..Default::default()
    };
    let before_delete = dal.query(ctx.clone(), query.clone()).await?;
    assert_eq!(before_delete.len(), 1);

    // 执行删除
    let memory = Memory::new(MemoryPo::ShortTerm(index));
    dal.delete(ctx.clone(), memory).await?;

    // 验证删除后状态为 Forgotten（软删除）
    let after_delete = dal.query(ctx, query).await?;
    // 默认不包含 Forgotten 状态的记录
    assert_eq!(after_delete.len(), 0);

    Ok(())
}

#[sqlx::test]
async fn test_delete_knowledge_node(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建知识节点
    let now = chrono::Utc::now().timestamp();
    let node = LongTermKnowledgeNodePo {
        id: "kn-delete-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "待删除的知识节点".to_string(),
        node_description: "待删除的描述".to_string(),
        node_type: "concept".to_string(),
        summary: "待删除的总结".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };

    let params = MemoryCreateParams::CreateKnowledgeNode {
        node: node.clone(),
        references: vec![],
    };
    dal.create(ctx.clone(), params).await?;

    // 直接删除知识节点（测试主要验证删除操作不报错）
    let memory = Memory::new(MemoryPo::KnowledgeNode(node));
    dal.delete(ctx.clone(), memory).await?;

    // 验证查询 Forgotten 状态时能找到 1 条（软删除已生效，状态更新正确）
    let query_forgotten = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        status: Some(MemoryStatus::Forgotten),
        ..Default::default()
    };
    let after_delete = dal.query(ctx, query_forgotten).await?;
    assert_eq!(after_delete.len(), 1);

    Ok(())
}

#[sqlx::test]
async fn test_delete_trace_unsupported(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建 Trace 类型的 Memory
    let trace = MemoryTrace::new(
        "agent-001".to_string(),
        "log-001".to_string(),
        "user-001".to_string(),
        "org-001".to_string(),
        common::enums::MemoryRole::User,
        "测试内容".to_string(),
        None,
    );
    let memory = Memory::new(MemoryPo::Trace(trace));

    // 尝试删除应该返回错误
    let result = dal.delete(ctx, memory).await;
    assert!(result.is_err());
    assert!(format!("{:?}", result.unwrap_err()).contains("原始记忆 Trace 不可删除"));

    Ok(())
}

// ========== update 方法测试 ==========

#[sqlx::test]
async fn test_update_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建短期记忆
    let now = chrono::Utc::now().timestamp();
    let original_index = ShortTermMemoryIndexPo {
        id: "st-update-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "原始内容".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateShortTerm(original_index.clone()),
    )
    .await?;

    // 修改内容
    let mut updated_index = original_index.clone();
    updated_index.summary = "更新后的内容".to_string();
    updated_index.tags = "[\"重要\"]".to_string();

    let updated_memory = Memory::new(MemoryPo::ShortTerm(updated_index.clone()));
    let result = dal.update(ctx.clone(), updated_memory).await?;

    // 验证更新成功
    match &result.po {
        MemoryPo::ShortTerm(po) => {
            assert_eq!(po.summary, "更新后的内容");
            assert_eq!(po.tags, "[\"重要\"]");
        }
        _ => panic!("Expected ShortTerm"),
    }

    // 验证数据库中确实更新了
    let query_result = dal
        .query(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some("agent-001".to_string()),
                memory_type: Some(common::enums::MemoryType::ShortTerm),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(query_result.len(), 1);
    match &query_result[0].po {
        MemoryPo::ShortTerm(po) => {
            assert_eq!(po.summary, "更新后的内容");
        }
        _ => panic!("Expected ShortTerm"),
    }

    Ok(())
}

#[sqlx::test]
async fn test_update_knowledge_node(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 先创建知识节点
    let now = chrono::Utc::now().timestamp();
    let original_node = LongTermKnowledgeNodePo {
        id: "kn-update-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "原始节点名称".to_string(),
        node_description: "原始描述".to_string(),
        node_type: "concept".to_string(),
        summary: "原始总结".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };

    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: original_node.clone(),
            references: vec![],
        },
    )
    .await?;

    // 修改内容
    let mut updated_node = original_node.clone();
    updated_node.node_name = "更新后的节点名称".to_string();
    updated_node.node_description = "更新后的描述".to_string();
    updated_node.summary = "更新后的总结".to_string();

    let updated_memory = Memory::new(MemoryPo::KnowledgeNode(updated_node.clone()));
    let result = dal.update(ctx.clone(), updated_memory).await?;

    // 验证更新成功
    match &result.po {
        MemoryPo::KnowledgeNode(po) => {
            assert_eq!(po.node_name, "更新后的节点名称");
            assert_eq!(po.node_description, "更新后的描述");
            assert_eq!(po.summary, "更新后的总结");
        }
        _ => panic!("Expected KnowledgeNode"),
    }

    // 验证数据库中确实更新了
    let query_result = dal
        .query(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some("agent-001".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(query_result.len(), 1);
    match &query_result[0].po {
        MemoryPo::KnowledgeNode(po) => {
            assert_eq!(po.node_name, "更新后的节点名称");
        }
        _ => panic!("Expected KnowledgeNode"),
    }

    Ok(())
}

#[sqlx::test]
async fn test_update_trace_unsupported(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建 Trace 类型的 Memory
    let trace = MemoryTrace::new(
        "agent-001".to_string(),
        "log-001".to_string(),
        "user-001".to_string(),
        "org-001".to_string(),
        common::enums::MemoryRole::User,
        "原始内容".to_string(),
        None,
    );
    let memory = Memory::new(MemoryPo::Trace(trace));

    // 尝试更新应该返回错误
    let result = dal.update(ctx, memory).await;
    assert!(result.is_err());
    assert!(format!("{:?}", result.unwrap_err()).contains("原始记忆 Trace 不可修改"));

    Ok(())
}

#[sqlx::test]
async fn test_update_relation_unsupported(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建 Relation 类型的 Memory（使用 PO 直接构造）
    let now = chrono::Utc::now().timestamp();
    let relation = KnowledgeNodeRelationPo {
        id: "rel-001".to_string(),
        source_node_id: "node-a".to_string(),
        target_node_id: "node-b".to_string(),
        relation_type: "related".to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let memory = Memory::new(MemoryPo::Relation(relation));

    // 尝试更新应该返回错误
    let result = dal.update(ctx, memory).await;
    assert!(result.is_err());
    assert!(format!("{:?}", result.unwrap_err()).contains("记忆 Relation 不可修改，需删除后重建"));

    Ok(())
}
