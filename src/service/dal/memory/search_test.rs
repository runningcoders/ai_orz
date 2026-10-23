//! Memory DAL 查询侧测试：`query` 关系型查询（含 task / status 过滤）
//! 与 `search` 关键词、混合、向量阈值、关系搜索。

use super::test_support::*;
use crate::models::memory::{
    KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, MemoryCreateParams, MemoryPo,
    ShortTermMemoryIndexPo,
};
use crate::models::vector::MatchType;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use common::enums::{KnowledgeRelationStatus, MemoryStatus};
use common::error::Result;
use sqlx::SqlitePool;

// ========== query 方法测试 ==========

#[sqlx::test]
async fn test_query_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建短期记忆（直接写入数据库绕过 create 的向量依赖）
    let po = ShortTermMemoryIndexPo {
        id: "test-mem-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Hello, world!".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: 1234567890,
        updated_at: 1234567890,
    };
    sqlx::query(
        r#"
        INSERT INTO short_term_memory_index (id, agent_id, task_id, role, summary, tags, trace_ids, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&po.id)
    .bind(&po.agent_id)
    .bind::<Option<&str>>(None)
    .bind(&po.role)
    .bind(&po.summary)
    .bind(&po.tags)
    .bind(&po.trace_ids)
    .bind(po.status as i32)
    .bind(po.created_at)
    .bind(po.updated_at)
    .execute(ctx.db_pool())
    .await?;

    // 查询测试
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        ..Default::default()
    };
    let results = dal.query(ctx, query).await?;

    assert!(!results.is_empty());

    Ok(())
}

/// 测试 query_short_term 的 task_id 过滤（注意力机制）
///
/// 创建 3 条短期记忆：2 条带不同 task_id，1 条不带 task_id
/// 验证：
/// - 不传 task_id → 返回全部（默认跨任务全局取）
/// - 传 task_id → 只返回该 task 的记忆
/// - 传不存在的 task_id → 返回空
#[sqlx::test]
async fn test_query_short_term_by_task_id(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    let now = chrono::Utc::now().timestamp();
    let agent_id = "agent-task-filter".to_string();

    // 创建 3 条记忆：task-A / task-B / 无 task
    let memories = [
        ("mem-task-a", Some("task-A".to_string()), "task A 的记忆"),
        ("mem-task-b", Some("task-B".to_string()), "task B 的记忆"),
        ("mem-no-task", None, "无任务关联的记忆"),
    ];

    for (i, (id, task_id, summary)) in memories.iter().enumerate() {
        let po = ShortTermMemoryIndexPo {
            id: id.to_string(),
            agent_id: agent_id.clone(),
            task_id: task_id.clone(),
            role: "user".to_string(),
            summary: summary.to_string(),
            tags: "[]".to_string(),
            trace_ids: "[]".to_string(),
            status: MemoryStatus::Active,
            created_at: now + i as i64,
            updated_at: now + i as i64,
        };
        sqlx::query(
            r#"INSERT INTO short_term_memory_index (id, agent_id, task_id, role, summary, tags, trace_ids, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&po.id)
        .bind(&po.agent_id)
        .bind(&po.task_id)
        .bind(&po.role)
        .bind(&po.summary)
        .bind(&po.tags)
        .bind(&po.trace_ids)
        .bind(po.status as i32)
        .bind(po.created_at)
        .bind(po.updated_at)
        .execute(ctx.db_pool())
        .await?;
    }

    // 1. 不传 task_id → 返回全部 3 条
    let query = MemoryQuery {
        agent_id: Some(agent_id.clone()),
        limit: Some(10),
        ..Default::default()
    };
    let results = dal.query(ctx.clone(), query).await?;
    assert_eq!(results.len(), 3, "不带 task_id 过滤应返回全部 3 条");

    // 2. 传 task_id=task-A → 只返回 1 条
    let query = MemoryQuery {
        agent_id: Some(agent_id.clone()),
        task_id: Some("task-A".to_string()),
        limit: Some(10),
        ..Default::default()
    };
    let results = dal.query(ctx.clone(), query).await?;
    assert_eq!(results.len(), 1, "task_id=task-A 应只返回 1 条");
    match &results[0].po {
        MemoryPo::ShortTerm(st) => assert_eq!(st.summary, "task A 的记忆"),
        _ => panic!("expected ShortTerm"),
    }

    // 3. 传 task_id=task-B → 只返回 1 条
    let query = MemoryQuery {
        agent_id: Some(agent_id.clone()),
        task_id: Some("task-B".to_string()),
        limit: Some(10),
        ..Default::default()
    };
    let results = dal.query(ctx.clone(), query).await?;
    assert_eq!(results.len(), 1, "task_id=task-B 应只返回 1 条");

    // 4. 传不存在的 task_id → 返回空
    let query = MemoryQuery {
        agent_id: Some(agent_id.clone()),
        task_id: Some("nonexistent-task".to_string()),
        limit: Some(10),
        ..Default::default()
    };
    let results = dal.query(ctx, query).await?;
    assert_eq!(results.len(), 0, "不存在的 task_id 应返回空");

    Ok(())
}

#[sqlx::test]
async fn test_query_with_status_filter(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建一个 Active 状态的记忆
    let po = ShortTermMemoryIndexPo {
        id: "test-mem-active".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Active memory".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: 1234567890,
        updated_at: 1234567890,
    };
    sqlx::query(
        r#"
        INSERT INTO short_term_memory_index (id, agent_id, task_id, role, summary, tags, trace_ids, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&po.id)
    .bind(&po.agent_id)
    .bind::<Option<&str>>(None)
    .bind(&po.role)
    .bind(&po.summary)
    .bind(&po.tags)
    .bind(&po.trace_ids)
    .bind(po.status as i32)
    .bind(po.created_at)
    .bind(po.updated_at)
    .execute(ctx.db_pool())
    .await?;

    // 创建一个 Deleted 状态的记忆
    let po2 = ShortTermMemoryIndexPo {
        id: "test-mem-deleted".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Deleted memory".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Forgotten,
        created_at: 1234567890,
        updated_at: 1234567890,
    };
    sqlx::query(
        r#"
        INSERT INTO short_term_memory_index (id, agent_id, task_id, role, summary, tags, trace_ids, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&po2.id)
    .bind(&po2.agent_id)
    .bind::<Option<&str>>(None)
    .bind(&po2.role)
    .bind(&po2.summary)
    .bind(&po2.tags)
    .bind(&po2.trace_ids)
    .bind(po2.status as i32)
    .bind(po2.created_at)
    .bind(po2.updated_at)
    .execute(ctx.db_pool())
    .await?;

    // 查询 Active 状态（默认）
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        ..Default::default()
    };
    let results = dal.query(ctx.clone(), query).await?;
    assert_eq!(results.len(), 1);

    // 查询 Deleted 状态
    let query = MemoryQuery {
        agent_id: Some("agent-001".to_string()),
        status: Some(MemoryStatus::Forgotten),
        ..Default::default()
    };
    let results = dal.query(ctx, query).await?;
    assert_eq!(results.len(), 1);

    Ok(())
}

#[sqlx::test]
async fn test_query_empty_result(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 查询不存在的 agent
    let query = MemoryQuery {
        agent_id: Some("non-existent-agent".to_string()),
        ..Default::default()
    };
    let results = dal.query(ctx, query).await?;

    assert!(results.is_empty());

    Ok(())
}

// ========== search 方法测试 ==========

#[sqlx::test]
async fn test_search_short_term(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建多条短期记忆
    let now = chrono::Utc::now().timestamp();

    let index1 = ShortTermMemoryIndexPo {
        id: "st-search-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "用户询问 Rust 编程基础概念".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let index2 = ShortTermMemoryIndexPo {
        id: "st-search-002".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "assistant".to_string(),
        summary: "Python 数据处理与机器学习入门指南".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    let index3 = ShortTermMemoryIndexPo {
        id: "st-search-003".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "assistant".to_string(),
        summary: "其他无关内容".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index1))
        .await?;
    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index2))
        .await?;
    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index3))
        .await?;

    // 搜索：匹配 "Rust" 关键词（只匹配第一条）
    let results = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("Rust".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results.len(), 1);

    // 搜索：匹配 "Python" 关键词（只匹配第二条）
    let results2 = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("Python".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results2.len(), 1);

    // 搜索：无匹配
    let results3 = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("nonexistent-keyword".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results3.len(), 0);

    Ok(())
}

/// 测试 search_short_term 的 task_id 过滤（FTS5 搜索 + task 聚焦）
#[sqlx::test]
async fn test_search_short_term_by_task_id(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    let now = chrono::Utc::now().timestamp();
    let agent_id = "agent-search-task".to_string();

    // 创建 2 条都包含 "Rust" 关键词但属于不同 task 的记忆
    for (id, task_id) in [("st-task-a", Some("task-A")), ("st-task-b", Some("task-B"))] {
        let po = ShortTermMemoryIndexPo {
            id: id.to_string(),
            agent_id: agent_id.clone(),
            task_id: task_id.map(|s| s.to_string()),
            role: "user".to_string(),
            summary: format!("Rust 编程在 {} 中的应用", task_id.unwrap_or("")),
            tags: "[]".to_string(),
            trace_ids: "[]".to_string(),
            status: MemoryStatus::Active,
            created_at: now,
            updated_at: now,
        };
        dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(po))
            .await?;
    }

    // 1. 不带 task_id 搜索 "Rust" → 返回 2 条
    let results = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("Rust".to_string()),
                filters: MemoryQuery {
                    agent_id: Some(agent_id.clone()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.len(), 2, "不带 task_id 应返回 2 条");

    // 2. 带 task_id=task-A 搜索 "Rust" → 只返回 1 条
    let results = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("Rust".to_string()),
                filters: MemoryQuery {
                    agent_id: Some(agent_id.clone()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    task_id: Some("task-A".to_string()),
                    limit: Some(10),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(results.len(), 1, "task_id=task-A 应只返回 1 条");

    Ok(())
}

#[sqlx::test]
async fn test_search_knowledge_node(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    // 创建多个知识节点
    let now = chrono::Utc::now().timestamp();

    let node1 = LongTermKnowledgeNodePo {
        id: "kn-search-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "Rust 所有权机制".to_string(),
        node_description: "深入理解 Rust 所有权、借用和生命周期概念".to_string(),
        node_type: "concept".to_string(),
        summary: "Rust 语言的核心特性之一，确保内存安全".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };

    let node2 = LongTermKnowledgeNodePo {
        id: "kn-search-002".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "Python decorator 详解".to_string(),
        node_description: "Python 高级特性：函数 decorator 原理与应用".to_string(),
        node_type: "concept".to_string(),
        summary: "Python decorator 装饰器是一种强大的元编程工具".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };

    let node3 = LongTermKnowledgeNodePo {
        id: "kn-search-003".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "其他内容".to_string(),
        node_description: "无关描述".to_string(),
        node_type: "other".to_string(),
        summary: "无关总结".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };

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
    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateKnowledgeNode {
            node: node3,
            references: vec![],
        },
    )
    .await?;

    // 搜索：按 node_name 匹配 "Rust"
    let results = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("Rust".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::KnowledgeNode),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results.len(), 1);

    // 搜索：按 summary 匹配 "装饰器"（trigram 分词器支持中文搜索）
    let results2 = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("装饰器".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::KnowledgeNode),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results2.len(), 1);

    // 搜索：无匹配
    let results3 = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("nonexistent-keyword".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::KnowledgeNode),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results3.len(), 0);

    Ok(())
}

#[sqlx::test]
async fn test_search_keyword_match_info(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool);

    // 创建短期记忆
    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-kw-match-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Rust 编程语言入门教程".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index))
        .await?;

    // 纯关键词搜索（无向量，MockModelProviderDao 返回 None）
    let results = dal
        .search(
            ctx,
            MemorySearch {
                keyword: Some("Rust".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;

    assert_eq!(results.len(), 1);
    let match_info = results[0]
        .search_match
        .as_ref()
        .expect("should have search_match");
    assert_eq!(match_info.match_type, MatchType::Keyword);
    assert!(
        match_info.fts_rank.is_some(),
        "fts_rank should be Some for keyword match"
    );
    assert!(
        match_info.vector_distance.is_none(),
        "vector_distance should be None for keyword-only match"
    );

    Ok(())
}

#[sqlx::test]
async fn test_search_hybrid_ranking(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;

    // Mock 向量搜索返回 st-hybrid-001 的命中（distance=0.5）
    let dal = init_test_with_vector(
        pool.clone(),
        vec![("st-hybrid-001".to_string(), 0.5)],
        vec![],
    )
    .await;
    let ctx = create_test_ctx(pool);

    let now = chrono::Utc::now().timestamp();

    // 创建两条短期记忆（都匹配关键词 "Rust"）
    let index1 = ShortTermMemoryIndexPo {
        id: "st-hybrid-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Rust 编程语言教程".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let index2 = ShortTermMemoryIndexPo {
        id: "st-hybrid-002".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "assistant".to_string(),
        summary: "Rust 并发编程指南".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now + 1,
        updated_at: now + 1,
    };
    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index1))
        .await?;
    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index2))
        .await?;

    // 搜索 "Rust"：两条都匹配关键词，但只有 st-hybrid-001 在向量结果中
    let results = dal
        .search(
            ctx,
            MemorySearch {
                keyword: Some("Rust".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;

    assert_eq!(results.len(), 2, "should return 2 results");

    // Hybrid 应排在第一位
    let first = &results[0];
    let first_match = first.search_match.as_ref().expect("should have match");
    assert_eq!(
        first_match.match_type,
        MatchType::Hybrid,
        "first result should be Hybrid"
    );
    assert!(
        first_match.vector_distance.is_some(),
        "Hybrid should have vector_distance"
    );
    assert!(
        first_match.fts_rank.is_some(),
        "Hybrid should have fts_rank"
    );
    match &first.po {
        MemoryPo::ShortTerm(st) => assert_eq!(st.id, "st-hybrid-001"),
        _ => panic!("Expected ShortTerm"),
    }

    // Keyword 应排在第二位
    let second = &results[1];
    let second_match = second.search_match.as_ref().expect("should have match");
    assert_eq!(
        second_match.match_type,
        MatchType::Keyword,
        "second result should be Keyword"
    );
    assert!(
        second_match.vector_distance.is_none(),
        "Keyword-only should not have vector_distance"
    );

    Ok(())
}

#[sqlx::test]
async fn test_search_vector_distance_threshold(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;

    // Mock 向量搜索返回 distance=0.9（超过默认阈值 0.8）
    let dal = init_test_with_vector(
        pool.clone(),
        vec![("st-threshold-001".to_string(), 0.9)],
        vec![],
    )
    .await;
    let ctx = create_test_ctx(pool.clone());

    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-threshold-001".to_string(),
        agent_id: "agent-001".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Rust 内存安全机制".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dal.create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index))
        .await?;

    // 默认阈值 0.8：向量结果（distance=0.9）被过滤，只有关键词命中
    let results_default = dal
        .search(
            ctx.clone(),
            MemorySearch {
                keyword: Some("Rust".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None, // 默认 0.8
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results_default.len(), 1);
    let default_match = results_default[0]
        .search_match
        .as_ref()
        .expect("should have match");
    assert_eq!(
        default_match.match_type,
        MatchType::Keyword,
        "with default threshold 0.8, distance 0.9 should be filtered → Keyword only"
    );

    // 自定义阈值 1.0：向量结果保留 → Hybrid
    let results_custom = dal
        .search(
            ctx,
            MemorySearch {
                keyword: Some("Rust".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: Some(1.0),
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(10),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(results_custom.len(), 1);
    let custom_match = results_custom[0]
        .search_match
        .as_ref()
        .expect("should have match");
    assert_eq!(
        custom_match.match_type,
        MatchType::Hybrid,
        "with threshold 1.0, distance 0.9 should be kept → Hybrid"
    );
    assert!(
        custom_match.vector_distance.is_some(),
        "Hybrid should have vector_distance"
    );

    Ok(())
}

#[sqlx::test]
async fn test_search_relations(pool: SqlitePool) -> Result<()> {
    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let ctx = create_test_ctx(pool.clone());

    let now = chrono::Utc::now().timestamp();

    // 创建两个知识节点（描述包含 "Rust"）
    let node1 = LongTermKnowledgeNodePo {
        id: "kn-rel-001".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "Rust 所有权机制".to_string(),
        node_description: "Rust 所有权、借用和生命周期".to_string(),
        node_type: "concept".to_string(),
        summary: "Rust 内存安全核心特性".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };
    let node2 = LongTermKnowledgeNodePo {
        id: "kn-rel-002".to_string(),
        agent_id: "agent-001".to_string(),
        node_name: "Rust 借用检查器".to_string(),
        node_description: "Rust 编译期借用检查".to_string(),
        node_type: "concept".to_string(),
        summary: "Rust 借用规则与检查".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };
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
        id: "rel-search-001".to_string(),
        source_node_id: "kn-rel-001".to_string(),
        target_node_id: "kn-rel-002".to_string(),
        relation_type: "related".to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dal.create(
        ctx.clone(),
        MemoryCreateParams::CreateRelations(vec![relation]),
    )
    .await?;

    // 搜索关系（memory_type=Relation，触发 search_relations_internal）
    let results = dal
        .search(
            ctx,
            MemorySearch {
                keyword: Some("Rust".to_string()),
                query_vector: None,
                top_k: None,
                vector_distance_threshold: None,
                filters: MemoryQuery {
                    agent_id: Some("agent-001".to_string()),
                    memory_type: Some(common::enums::MemoryType::Relation),
                    limit: Some(20),
                    ..Default::default()
                },
            },
        )
        .await?;

    // 应返回知识节点（2 个）+ 关系（1 条）
    assert!(
        results.len() >= 3,
        "should return at least 2 nodes + 1 relation, got {}",
        results.len()
    );

    // 验证包含 KnowledgeNode 和 Relation 类型
    let has_node = results
        .iter()
        .any(|m| matches!(m.po, MemoryPo::KnowledgeNode(_)));
    let has_relation = results
        .iter()
        .any(|m| matches!(m.po, MemoryPo::Relation(_)));
    assert!(has_node, "should contain KnowledgeNode");
    assert!(has_relation, "should contain Relation");

    // 验证关系连接正确
    let rel = results
        .iter()
        .find_map(|m| match &m.po {
            MemoryPo::Relation(r) => Some(r),
            _ => None,
        })
        .expect("should find a Relation");
    assert_eq!(rel.source_node_id, "kn-rel-001");
    assert_eq!(rel.target_node_id, "kn-rel-002");

    // 验证知识节点的 search_match 为 Keyword
    let node_with_match = results
        .iter()
        .find_map(|m| match (&m.po, &m.search_match) {
            (MemoryPo::KnowledgeNode(_), Some(sm)) => Some(sm),
            _ => None,
        })
        .expect("should find a KnowledgeNode with search_match");
    assert_eq!(node_with_match.match_type, MatchType::Keyword);
    assert!(node_with_match.fts_rank.is_some());

    Ok(())
}
