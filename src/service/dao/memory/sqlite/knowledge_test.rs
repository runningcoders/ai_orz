//! Memory DAO 长期知识节点（knowledge）单元测试：节点 CRUD、查询与记忆引用

use crate::models::memory::{KnowledgeReferencePo, LongTermKnowledgeNodePo};
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use common::enums::MemoryStatus;
use sqlx::SqlitePool;

#[sqlx::test]
async fn test_create_knowledge_node(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    // 自动迁移已经由 sqlx::test 执行
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    // 测试插入知识节点 SQL 语法正确
    let node = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "Rust 内存安全".to_string(),
        node_description: "Rust 的内存安全特性".to_string(),
        node_type: "concept".to_string(),
        summary: "Rust 通过所有权系统实现内存安全".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    };

    let result = dao.save_knowledge_node(ctx.clone(), &node).await;
    assert!(result.is_ok());

    // 查询验证插入成功
    let dao = MemoryDaoSqliteImpl::new();
    let ctx2 = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    let fetched = dao.get_knowledge_node(ctx2, "node-1").await;
    assert!(fetched.is_ok());
    let fetched = fetched.unwrap();
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.id, "node-1");
    assert_eq!(fetched.node_name, "Rust 内存安全");
}

#[sqlx::test]
async fn test_add_knowledge_reference(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    // 先创建节点
    let node = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "测试节点".to_string(),
        node_description: "测试描述".to_string(),
        node_type: "test".to_string(),
        summary: "测试摘要".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    };
    dao.save_knowledge_node(ctx.clone(), &node).await.unwrap();

    // 添加引用
    let reference = KnowledgeReferencePo {
        id: "ref-1".to_string(),
        knowledge_id: "node-1".to_string(),
        short_term_id: "st-1".to_string(),
        trace_id: "trace-1".to_string(),
        date_path: "20260411.jsonl".to_string(),
        line_number: 0,
        created_at: 0,
    };

    let result = dao.add_knowledge_reference(ctx.clone(), &reference).await;
    assert!(result.is_ok());

    // 查询验证
    let references = dao
        .list_knowledge_references(ctx.clone(), "node-1")
        .await
        .unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].knowledge_id, "node-1");
    assert_eq!(references[0].short_term_id, "st-1");
}

#[sqlx::test]
async fn test_update_and_list_knowledge_nodes(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    let node = LongTermKnowledgeNodePo {
        id: "node-update".to_string(),
        agent_id: "test-agent".to_string(),
        node_name: "测试节点".to_string(),
        node_description: "原始描述".to_string(),
        node_type: "concept".to_string(),
        summary: "原始摘要".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };
    dao.save_knowledge_node(ctx.clone(), &node).await.unwrap();

    // 测试 update_knowledge_node
    let mut updated_node = node.clone();
    updated_node.summary = "更新后的摘要".to_string();
    dao.update_knowledge_node(ctx.clone(), &updated_node)
        .await
        .unwrap();

    let fetched = dao
        .get_knowledge_node(ctx.clone(), "node-update")
        .await
        .unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().summary, "更新后的摘要");

    // 测试 batch_save_knowledge_nodes
    let nodes = vec![
        LongTermKnowledgeNodePo {
            id: "node-batch-1".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "批量节点1".to_string(),
            node_description: "批量描述1".to_string(),
            node_type: "concept".to_string(),
            summary: "批量摘要1".to_string(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: now,
            updated_at: now,
        },
        LongTermKnowledgeNodePo {
            id: "node-batch-2".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "批量节点2".to_string(),
            node_description: "批量描述2".to_string(),
            node_type: "concept".to_string(),
            summary: "批量摘要2".to_string(),
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

    // 测试 list_knowledge_nodes_by_agent
    let list = dao
        .list_knowledge_nodes_by_agent(ctx.clone(), "test-agent", None, 10)
        .await
        .unwrap();
    assert_eq!(list.len(), 3); // node-update, node-batch-1, node-batch-2
}

#[sqlx::test]
async fn test_query_and_delete_knowledge_nodes(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    // 创建几个节点
    let nodes = vec![
        LongTermKnowledgeNodePo {
            id: "query-1".to_string(),
            agent_id: "agent-a".to_string(),
            node_name: "Rust".to_string(),
            node_description: "Rust 语言学习笔记".to_string(),
            node_type: "language".to_string(),
            summary: "Rust 是一门系统编程语言".to_string(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: now,
            updated_at: now,
        },
        LongTermKnowledgeNodePo {
            id: "query-2".to_string(),
            agent_id: "agent-a".to_string(),
            node_name: "Python".to_string(),
            node_description: "Python 编程技巧".to_string(),
            node_type: "language".to_string(),
            summary: "Python 是一门动态语言".to_string(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: now,
            updated_at: now,
        },
        LongTermKnowledgeNodePo {
            id: "query-3".to_string(),
            agent_id: "agent-b".to_string(),
            node_name: "机器学习".to_string(),
            node_description: "机器学习入门".to_string(),
            node_type: "concept".to_string(),
            summary: "机器学习是人工智能的基础".to_string(),
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

    // 测试 query_knowledge_nodes 按 agent_id
    use crate::service::dao::memory::MemoryQuery;
    let query = MemoryQuery {
        agent_id: Some("agent-a".to_string()),
        ..Default::default()
    };
    let results = dao.query_knowledge_nodes(ctx.clone(), query).await.unwrap();
    assert_eq!(results.len(), 2); // query-1, query-2

    // 测试 search_knowledge_nodes 关键词搜索 - 注意：需要 FTS 虚拟表支持
    // 当前 SQLite 表未启用 FTS，跳过此测试
    // let search = MemorySearch {
    //     keyword: Some("Rust".to_string()),
    //     filters: MemoryQuery {
    //         agent_id: Some("agent-a".to_string()),
    //         ..Default::default()
    //     },
    //     ..Default::default()
    // };
    // let search_results = dao.search_knowledge_nodes(ctx.clone(), search).await.unwrap();
    // assert_eq!(search_results.len(), 1); // query-1

    // 测试 delete_knowledge_node
    dao.delete_knowledge_node(ctx.clone(), "query-1")
        .await
        .unwrap();
    let deleted = dao
        .get_knowledge_node(ctx.clone(), "query-1")
        .await
        .unwrap();
    assert!(deleted.is_none());
}

#[sqlx::test]
async fn test_knowledge_references(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    // 创建节点
    let node = LongTermKnowledgeNodePo {
        id: "ref-node".to_string(),
        agent_id: "test-agent".to_string(),
        node_name: "引用测试节点".to_string(),
        node_description: "引用测试描述".to_string(),
        node_type: "test".to_string(),
        summary: "引用测试摘要".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };
    dao.save_knowledge_node(ctx.clone(), &node).await.unwrap();

    // 测试 batch_add_knowledge_references
    let references = vec![
        KnowledgeReferencePo {
            id: "ref-1".to_string(),
            knowledge_id: "ref-node".to_string(),
            short_term_id: "st-1".to_string(),
            trace_id: "trace-1".to_string(),
            date_path: "20260411.jsonl".to_string(),
            line_number: 0,
            created_at: now,
        },
        KnowledgeReferencePo {
            id: "ref-2".to_string(),
            knowledge_id: "ref-node".to_string(),
            short_term_id: "st-2".to_string(),
            trace_id: "trace-2".to_string(),
            date_path: "20260411.jsonl".to_string(),
            line_number: 1,
            created_at: now,
        },
    ];
    dao.batch_add_knowledge_references(ctx.clone(), &references)
        .await
        .unwrap();

    // 测试 list_knowledge_references
    let refs = dao
        .list_knowledge_references(ctx.clone(), "ref-node")
        .await
        .unwrap();
    assert_eq!(refs.len(), 2);
}

#[sqlx::test]
async fn test_search_knowledge_nodes_fts5(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let now = chrono::Utc::now().timestamp();

    // 创建知识节点
    let node1 = LongTermKnowledgeNodePo {
        id: "kn-fts-search-1".to_string(),
        agent_id: "agent-kn-fts".to_string(),
        node_name: "Rust ownership mechanism".to_string(),
        node_description: "borrow checker ensures safety".to_string(),
        node_type: "concept".to_string(),
        summary: "ownership system is core to Rust language".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };
    dao.save_knowledge_node(ctx.clone(), &node1).await.unwrap();

    let node2 = LongTermKnowledgeNodePo {
        id: "kn-fts-search-2".to_string(),
        agent_id: "agent-kn-fts".to_string(),
        node_name: "Python decorator".to_string(),
        node_description: "function decorator pattern".to_string(),
        node_type: "concept".to_string(),
        summary: "decorator is a metaprogramming tool in Python".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: now,
        updated_at: now,
    };
    dao.save_knowledge_node(ctx.clone(), &node2).await.unwrap();

    // 搜索 "rust"
    use crate::service::dao::memory::{MemoryQuery, MemorySearch};
    let search = MemorySearch {
        keyword: Some("rust".to_string()),
        filters: MemoryQuery {
            agent_id: Some("agent-kn-fts".to_string()),
            limit: Some(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let results = dao.search_knowledge_nodes(ctx, search).await.unwrap();

    // 应只匹配第一个节点
    assert_eq!(results.len(), 1);
    let (po, fts_rank) = &results[0];
    assert_eq!(po.id, "kn-fts-search-1");
    assert!(po.node_name.contains("Rust"));
    assert!(
        fts_rank.is_some(),
        "fts_rank should be Some for MATCH results"
    );
}

#[sqlx::test]
async fn test_query_knowledge_nodes_tags_filter(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    // 插入 3 个知识节点，带不同 tags
    let nodes = vec![
        LongTermKnowledgeNodePo {
            id: "kn-tags-1".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "Rust 基础".to_string(),
            node_description: "Rust 所有权与借用".to_string(),
            node_type: "concept".to_string(),
            summary: "Rust 内存安全".to_string(),
            tags: r#"["rust","memory"]"#.to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: 1000,
            updated_at: 1000,
        },
        LongTermKnowledgeNodePo {
            id: "kn-tags-2".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "React Hooks".to_string(),
            node_description: "React 状态管理".to_string(),
            node_type: "concept".to_string(),
            summary: "前端状态".to_string(),
            tags: r#"["react","frontend"]"#.to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: 2000,
            updated_at: 2000,
        },
        LongTermKnowledgeNodePo {
            id: "kn-tags-3".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "WASM 互操作".to_string(),
            node_description: "Rust 与 JS 互操作".to_string(),
            node_type: "concept".to_string(),
            summary: "Rust 编译到 WASM".to_string(),
            tags: r#"["rust","frontend"]"#.to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: 3000,
            updated_at: 3000,
        },
    ];
    dao.batch_save_knowledge_nodes(ctx.clone(), &nodes)
        .await
        .unwrap();

    use crate::service::dao::memory::MemoryQuery;

    // 按 "rust" tag 过滤 → 应返回 node1 和 node3
    let query_rust = MemoryQuery {
        agent_id: Some("test-agent".to_string()),
        tags: Some(vec!["rust".to_string()]),
        ..Default::default()
    };
    let results = dao
        .query_knowledge_nodes(ctx.clone(), query_rust)
        .await
        .unwrap();
    assert_eq!(results.len(), 2, "按 rust tag 过滤应返回 2 个节点");
    let ids: Vec<&str> = results.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains(&"kn-tags-1"));
    assert!(ids.contains(&"kn-tags-3"));

    // 按 "frontend" tag 过滤 → 应返回 node2 和 node3
    let query_frontend = MemoryQuery {
        agent_id: Some("test-agent".to_string()),
        tags: Some(vec!["frontend".to_string()]),
        ..Default::default()
    };
    let results = dao
        .query_knowledge_nodes(ctx.clone(), query_frontend)
        .await
        .unwrap();
    assert_eq!(results.len(), 2, "按 frontend tag 过滤应返回 2 个节点");
    let ids: Vec<&str> = results.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains(&"kn-tags-2"));
    assert!(ids.contains(&"kn-tags-3"));

    // 按 OR 语义过滤 "rust" + "react" → 应返回全部 3 个
    let query_multi = MemoryQuery {
        agent_id: Some("test-agent".to_string()),
        tags: Some(vec!["rust".to_string(), "react".to_string()]),
        ..Default::default()
    };
    let results = dao
        .query_knowledge_nodes(ctx.clone(), query_multi)
        .await
        .unwrap();
    assert_eq!(results.len(), 3, "按 rust+react OR 语义过滤应返回 3 个节点");

    // 无 tags 过滤 → 应返回全部 3 个
    let query_none = MemoryQuery {
        agent_id: Some("test-agent".to_string()),
        ..Default::default()
    };
    let results = dao.query_knowledge_nodes(ctx, query_none).await.unwrap();
    assert_eq!(results.len(), 3, "无 tags 过滤应返回全部 3 个节点");
}
