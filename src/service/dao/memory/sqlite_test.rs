//! Memory DAO 单元测试
//!
//! 单元测试使用内存数据库，不依赖全局 storage 连接池

use super::*;
use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgeReferencePo, LongTermKnowledgeNodePo, MemoryTrace,
    ShortTermMemoryIndexPo,
};
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use common::enums::{KnowledgeRelationType, MemoryRole, MemoryStatus};
use sqlx::{Row, SqlitePool};

#[sqlx::test]
async fn test_append_trace_and_create_short_term_index(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    // 自动迁移已经由 sqlx::test 执行
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        "test-log-1".to_string(),
        "test-user".to_string(),
        "test-org".to_string(),
        MemoryRole::User,
        "这是一段测试内容".to_string(),
        None, // 测试不需要 task_id
    );

    // 阶段 1：append trace 到 daily jsonl
    let position = dao.append_trace(ctx.clone(), &trace).await.unwrap();
    assert_eq!(position.trace_id, trace.id);
    assert!(position.date_filename.ends_with(".jsonl"));

    // 阶段 2：创建 short-term index 关联 trace
    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "测试摘要".to_string(),
        tags: serde_json::to_string(&vec!["test", "memory"]).unwrap(),
        trace_ids: serde_json::to_string(&vec![&position.trace_id]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let result = dao.create_short_term_index(ctx, index).await;
    assert!(result.is_ok());
}

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
        created_at: 0,
        updated_at: 0,
    };

    dao.save_knowledge_node(ctx.clone(), &node1).await.unwrap();
    dao.save_knowledge_node(ctx.clone(), &node2).await.unwrap();

    // 添加关系
    let relation = KnowledgeNodeRelationPo {
        id: "rel-1".to_string(),
        source_node_id: "node-1".to_string(),
        target_node_id: "node-2".to_string(),
        relation_type: KnowledgeRelationType::Related,
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

#[test]
fn test_memory_trace_id_is_trace_prefix() {
    // 验证 MemoryTrace 的 ID 格式是 trace-{agent_id}-{timestamp}
    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        "log-1".to_string(),
        "user-1".to_string(),
        "org-1".to_string(),
        MemoryRole::User,
        "这是一段测试内容".to_string(),
        None,
    );

    // ID 应该以 trace- 开头
    assert!(trace.id.starts_with("trace-"));

    // 应该包含 agent_id
    assert!(trace.id.contains("test-agent-1"));

    // 最后一部分应该是数字（timestamp）
    let parts: Vec<&str> = trace.id.rsplitn(2, '-').collect();
    assert!(parts[0].parse::<u64>().is_ok());
}

#[test]
fn test_memory_trace_to_markdown() {
    // 验证 MemoryTrace 可以正确格式化为 markdown
    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        "log-1".to_string(),
        "user-1".to_string(),
        "org-1".to_string(),
        MemoryRole::User,
        "你好，这是一个测试问题".to_string(),
        None,
    );

    let markdown = trace.to_markdown();
    assert!(markdown.contains(&trace.id));
    assert!(markdown.contains("User"));
    assert!(markdown.contains("你好，这是一个测试问题"));
}

#[sqlx::test]
async fn test_batch_append_traces(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let traces = vec![
        MemoryTrace::new(
            "test-agent-1".to_string(),
            "test-log-1".to_string(),
            "test-user".to_string(),
            "test-org".to_string(),
            MemoryRole::User,
            "第一条测试内容".to_string(),
            None,
        ),
        MemoryTrace::new(
            "test-agent-1".to_string(),
            "test-log-1".to_string(),
            "test-user".to_string(),
            "test-org".to_string(),
            MemoryRole::Assistant,
            "第二条测试内容".to_string(),
            None,
        ),
        MemoryTrace::new(
            "test-agent-1".to_string(),
            "test-log-1".to_string(),
            "test-user".to_string(),
            "test-org".to_string(),
            MemoryRole::User,
            "第三条测试内容".to_string(),
            None,
        ),
    ];

    // 批量追加
    let positions = dao.batch_append_traces(ctx, &traces).await.unwrap();
    assert_eq!(positions.len(), 3);
    assert_eq!(positions[0].trace_id, traces[0].id);
    assert_eq!(positions[1].trace_id, traces[1].id);
    assert_eq!(positions[2].trace_id, traces[2].id);
    assert!(positions[0].date_filename.ends_with(".jsonl"));
}

#[sqlx::test]
async fn test_get_and_update_short_term_index(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "测试摘要".to_string(),
        tags: serde_json::to_string(&vec!["test", "memory"]).unwrap(),
        trace_ids: serde_json::to_string(&vec!["trace-1"]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    // 创建
    dao.create_short_term_index(ctx.clone(), index)
        .await
        .unwrap();

    // 查询
    let fetched = dao.get_short_term_index(ctx.clone(), "st-1").await.unwrap();
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.id, "st-1");
    assert_eq!(fetched.summary, "测试摘要");

    // 更新
    let now2 = chrono::Utc::now().timestamp();
    let updated_index = ShortTermMemoryIndexPo {
        id: "st-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "更新后的摘要".to_string(),
        tags: serde_json::to_string(&vec!["test", "memory", "updated"]).unwrap(),
        trace_ids: serde_json::to_string(&vec!["trace-1", "trace-2"]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now2,
    };

    dao.update_short_term_index(ctx.clone(), updated_index)
        .await
        .unwrap();

    // 验证更新
    let fetched2 = dao.get_short_term_index(ctx, "st-1").await.unwrap();
    assert!(fetched2.is_some());
    let fetched2 = fetched2.unwrap();
    assert_eq!(fetched2.summary, "更新后的摘要");
    assert!(fetched2.tags.contains("updated"));
}

#[sqlx::test]
async fn test_list_and_query_short_term(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();

    // 创建多个短期记忆
    for i in 0..5 {
        let index = ShortTermMemoryIndexPo {
            id: format!("st-{}", i),
            agent_id: if i < 3 {
                "agent-1".to_string()
            } else {
                "agent-2".to_string()
            },
            task_id: None,
            role: "user".to_string(),
            summary: format!("摘要 {}", i),
            tags: serde_json::to_string(&vec!["test"]).unwrap(),
            trace_ids: serde_json::to_string(&vec![format!("trace-{}", i)]).unwrap(),
            status: if i == 0 {
                MemoryStatus::Forgotten
            } else {
                MemoryStatus::Active
            },
            created_at: now,
            updated_at: now,
        };
        dao.create_short_term_index(ctx.clone(), index)
            .await
            .unwrap();
    }

    // 测试 list_short_term_by_agent
    let list = dao
        .list_short_term_by_agent(ctx.clone(), "agent-1", 10)
        .await
        .unwrap();
    assert_eq!(list.len(), 2); // 默认过滤 status=0 (Forgotten)，所以只有 st-1, st-2

    // 测试 query_short_term 按 agent_id + 排除状态
    use crate::service::dao::memory::MemoryQuery;

    let query = MemoryQuery {
        agent_id: Some("agent-1".to_string()),
        exclude_status: Some(MemoryStatus::Forgotten),
        ..Default::default()
    };
    let result = dao.query_short_term(ctx.clone(), query).await.unwrap();
    assert_eq!(result.len(), 2); // st-1, st-2 (排除了 Forgotten)

    // 测试不带 exclude_status 的默认查询
    let query2 = MemoryQuery {
        agent_id: Some("agent-1".to_string()),
        ..Default::default()
    };
    let result2 = dao.query_short_term(ctx, query2).await.unwrap();
    assert_eq!(result2.len(), 2); // 默认也排除了 status=0 (Forgotten)
}

#[sqlx::test]
async fn test_forget_short_term_index(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-forget".to_string(),
        agent_id: "test-agent".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "要被遗忘的记忆".to_string(),
        tags: serde_json::to_string(&vec!["test"]).unwrap(),
        trace_ids: serde_json::to_string(&vec!["trace-1"]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    // 创建
    dao.create_short_term_index(ctx.clone(), index)
        .await
        .unwrap();

    // 遗忘（软删除）
    dao.forget_short_term_index(ctx.clone(), "st-forget")
        .await
        .unwrap();

    // 验证无法再通过 get_short_term_index 获取（软删除过滤）
    let fetched = dao
        .get_short_term_index(ctx.clone(), "st-forget")
        .await
        .unwrap();
    assert!(
        fetched.is_none(),
        "软删除后无法通过 get_short_term_index 获取"
    );

    // 验证可以通过 query_short_term 获取（设置 exclude_status=Some(MemoryStatus::Active) 只排除 Active，不排除 Forgotten）
    use crate::service::dao::memory::MemoryQuery;
    let query = MemoryQuery {
        ids: Some(vec!["st-forget".to_string()]),
        exclude_status: Some(MemoryStatus::Active),
        ..Default::default()
    };
    let results = dao.query_short_term(ctx, query).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].status, MemoryStatus::Forgotten);
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
        relation_type: KnowledgeRelationType::Related,
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

    // 测试 delete_knowledge_relation (按 relation_id)
    dao.delete_knowledge_relation(ctx.clone(), "rel-rel-1-2")
        .await
        .unwrap();
    let outgoing_after = dao
        .list_outgoing_relations(ctx.clone(), "rel-1")
        .await
        .unwrap();
    assert_eq!(outgoing_after.len(), 0);
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

// ==================== FTS5 触发器同步测试 ====================
// 注意：unicode61 分词器将连续 CJK 字符视为单个 token，因此 MATCH 搜索
// 使用英文关键词验证。中文内容同步通过直接 SELECT FTS 表验证。

#[sqlx::test]
async fn test_fts5_trigger_insert_sync(pool: SqlitePool) {
    // 初始化配置，迁移由 sqlx::test 自动运行（含 FTS5 虚拟表 + 触发器）
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-fts-ins".to_string(),
        agent_id: "test-agent".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Rust ownership system 内存安全".to_string(),
        tags: serde_json::to_string(&vec!["rust", "memory"]).unwrap(),
        trace_ids: serde_json::to_string(&vec!["trace-1"]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };

    // 插入短期记忆 —— AFTER INSERT 触发器应自动写入 FTS
    dao.create_short_term_index(ctx, index).await.unwrap();

    // 1. FTS 表应该有 1 条记录
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM short_term_memory_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // 2. 直接查询 FTS 表验证内容同步（含中文）
    let row = sqlx::query("SELECT rowid, summary, tags FROM short_term_memory_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    let summary: String = row.get("summary");
    let tags: String = row.get("tags");
    let rowid: i64 = row.get("rowid");
    assert!(summary.contains("Rust"));
    assert!(summary.contains("内存安全"), "FTS summary 应包含中文内容");
    assert!(tags.contains("rust"));
    assert!(rowid > 0);

    // 3. 通过 summary MATCH 搜索英文关键词
    let match_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM short_term_memory_fts WHERE summary MATCH ?")
            .bind("rust")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(match_count, 1, "MATCH rust 应命中");

    // 4. 通过 tags MATCH 搜索英文关键词
    let tags_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM short_term_memory_fts WHERE tags MATCH ?")
            .bind("rust")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tags_count, 1, "tags MATCH rust 应命中");
}

#[sqlx::test]
async fn test_fts5_trigger_update_sync(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-fts-upd".to_string(),
        agent_id: "test-agent".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Rust programming language".to_string(),
        tags: serde_json::to_string(&vec!["rust"]).unwrap(),
        trace_ids: serde_json::to_string(&vec!["trace-1"]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dao.create_short_term_index(ctx.clone(), index)
        .await
        .unwrap();

    // 更新 summary：Rust -> Python（AFTER UPDATE 触发器先删旧 FTS 条目再插新条目）
    let updated_index = ShortTermMemoryIndexPo {
        id: "st-fts-upd".to_string(),
        agent_id: "test-agent".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Python programming language".to_string(),
        tags: serde_json::to_string(&vec!["python"]).unwrap(),
        trace_ids: serde_json::to_string(&vec!["trace-1"]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now + 1,
    };
    dao.update_short_term_index(ctx, updated_index)
        .await
        .unwrap();

    // FTS 表仍应只有 1 条记录（update 触发器先删后插，不是新增）
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM short_term_memory_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // 新关键词 Python 应能搜到
    let new_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM short_term_memory_fts WHERE summary MATCH ?")
            .bind("python")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(new_count, 1, "更新后应能搜到新关键词 python");

    // 旧关键词 rust 应搜不到（旧 FTS 条目已被触发器删除）
    let old_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM short_term_memory_fts WHERE summary MATCH ?")
            .bind("rust")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old_count, 0, "更新后旧关键词 rust 应已从 FTS 移除");
}

#[sqlx::test]
async fn test_fts5_trigger_delete_sync(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-fts-del".to_string(),
        agent_id: "test-agent".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Rust deletable memory entry".to_string(),
        tags: serde_json::to_string(&vec!["rust"]).unwrap(),
        trace_ids: serde_json::to_string(&vec!["trace-1"]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dao.create_short_term_index(ctx, index).await.unwrap();

    // 确认 FTS 已有 1 条记录
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM short_term_memory_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, 1);

    // 硬删除（注意：DAO 的 forget_short_term_index 是软删除=UPDATE，不会触发 AFTER DELETE）
    // 这里用原始 SQL 触发 AFTER DELETE 触发器
    sqlx::query("DELETE FROM short_term_memory_index WHERE id = ?")
        .bind("st-fts-del")
        .execute(&pool)
        .await
        .unwrap();

    // FTS 表中对应记录应已被触发器删除
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM short_term_memory_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, 0, "硬删除后 FTS 表应为空");

    // MATCH 搜索应搜不到
    let search: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM short_term_memory_fts WHERE summary MATCH ?")
            .bind("rust")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(search, 0, "删除后 MATCH 搜索应无结果");
}

#[sqlx::test]
async fn test_knowledge_node_fts5_trigger_insert_sync(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    let node = LongTermKnowledgeNodePo {
        id: "kn-fts-1".to_string(),
        agent_id: "test-agent".to_string(),
        node_name: "Rust memory safety".to_string(),
        node_description: "ownership borrow checker mechanism".to_string(),
        node_type: "concept".to_string(),
        summary: "ownership system ensures memory safety".to_string(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    // save_knowledge_node 对新节点走 INSERT 路径，触发 AFTER INSERT 触发器
    dao.save_knowledge_node(ctx, &node).await.unwrap();

    // FTS 表应该有 1 条记录
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_node_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // 通过 node_name MATCH 搜索
    let name_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_node_fts WHERE node_name MATCH ?")
            .bind("rust")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(name_count, 1, "node_name MATCH rust 应命中");

    // 通过 summary MATCH 搜索
    let summary_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_node_fts WHERE summary MATCH ?")
            .bind("ownership")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(summary_count, 1, "summary MATCH ownership 应命中");

    // 通过 node_description MATCH 搜索
    let desc_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM knowledge_node_fts WHERE node_description MATCH ?",
    )
    .bind("borrow")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(desc_count, 1, "node_description MATCH borrow 应命中");
}

// ==================== FTS5 MATCH 搜索测试（DAO 层） ====================

#[sqlx::test]
async fn test_search_short_term_fts5(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let now = chrono::Utc::now().timestamp();

    // 创建一条包含英文关键词的短期记忆
    let index = ShortTermMemoryIndexPo {
        id: "st-fts-search-1".to_string(),
        agent_id: "agent-fts-1".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Rust ownership system ensures memory safety".to_string(),
        tags: serde_json::to_string(&vec!["rust", "memory"]).unwrap(),
        trace_ids: serde_json::to_string(&vec!["trace-1"]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dao.create_short_term_index(ctx.clone(), index)
        .await
        .unwrap();

    // 创建一条不匹配的记忆
    let index2 = ShortTermMemoryIndexPo {
        id: "st-fts-search-2".to_string(),
        agent_id: "agent-fts-1".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Python data analysis tutorial".to_string(),
        tags: serde_json::to_string(&vec!["python"]).unwrap(),
        trace_ids: serde_json::to_string(&vec!["trace-2"]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dao.create_short_term_index(ctx.clone(), index2)
        .await
        .unwrap();

    // 搜索 "rust" 关键词
    use crate::service::dao::memory::{MemoryQuery, MemorySearch};
    let search = MemorySearch {
        keyword: Some("rust".to_string()),
        filters: MemoryQuery {
            agent_id: Some("agent-fts-1".to_string()),
            limit: Some(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let results = dao.search_short_term(ctx, search).await.unwrap();

    // 应只匹配第一条记忆
    assert_eq!(results.len(), 1);
    let (po, fts_rank) = &results[0];
    assert_eq!(po.id, "st-fts-search-1");
    assert!(po.summary.contains("Rust"));
    // fts_rank 应有值（BM25 评分）
    assert!(
        fts_rank.is_some(),
        "fts_rank should be Some for MATCH results"
    );
}

#[sqlx::test]
async fn test_search_short_term_fts5_bm25_ranking(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let now = chrono::Utc::now().timestamp();

    // 创建多条含 "rust" 关键词的记忆，出现频率不同
    // 第一条：rust 出现 3 次（高相关性）
    let index1 = ShortTermMemoryIndexPo {
        id: "st-bm25-1".to_string(),
        agent_id: "agent-bm25".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Rust Rust Rust programming language features".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dao.create_short_term_index(ctx.clone(), index1)
        .await
        .unwrap();

    // 第二条：rust 出现 1 次（低相关性）
    let index2 = ShortTermMemoryIndexPo {
        id: "st-bm25-2".to_string(),
        agent_id: "agent-bm25".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Introduction to Rust for beginners".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dao.create_short_term_index(ctx.clone(), index2)
        .await
        .unwrap();

    // 第三条：不含 rust（不应被返回）
    let index3 = ShortTermMemoryIndexPo {
        id: "st-bm25-3".to_string(),
        agent_id: "agent-bm25".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "Python machine learning guide".to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dao.create_short_term_index(ctx.clone(), index3)
        .await
        .unwrap();

    // 搜索 "rust"
    use crate::service::dao::memory::{MemoryQuery, MemorySearch};
    let search = MemorySearch {
        keyword: Some("rust".to_string()),
        filters: MemoryQuery {
            agent_id: Some("agent-bm25".to_string()),
            limit: Some(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let results = dao.search_short_term(ctx, search).await.unwrap();

    // 应返回 2 条结果（排除第三条）
    assert_eq!(results.len(), 2);

    // BM25 排序：rust 出现 3 次的应排在前面（rank 值越小越相关）
    let (po1, rank1) = &results[0];
    let (po2, rank2) = &results[1];
    assert_eq!(po1.id, "st-bm25-1", "高相关性的记忆应排在第一位");
    assert_eq!(po2.id, "st-bm25-2", "低相关性的记忆应排在第二位");

    // 验证 fts_rank 均有值且排序正确
    let r1 = rank1.expect("first result should have fts_rank");
    let r2 = rank2.expect("second result should have fts_rank");
    assert!(
        r1 <= r2,
        "BM25 rank of more relevant doc should be <= less relevant (r1={}, r2={})",
        r1,
        r2
    );
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
