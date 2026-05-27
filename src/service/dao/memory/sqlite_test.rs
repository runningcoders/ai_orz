//! Memory DAO 单元测试
//!
//! 单元测试使用内存数据库，不依赖全局 storage 连接池

use super::*;
use crate::models::memory::{MemoryTrace, ShortTermMemoryIndexPo, LongTermKnowledgeNodePo, KnowledgeNodeRelationPo, KnowledgeReferencePo};
use common::enums::{MemoryRole, KnowledgeRelationType, MemoryStatus};
use crate::pkg::RequestContext;
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use sqlx::SqlitePool;

#[sqlx::test]
async fn test_append_trace_and_create_short_term_index(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    // 自动迁移已经由 sqlx::test 执行
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = RequestContext::new_simple("test-user", pool);

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
    let ctx = RequestContext::new_simple("test-user", pool.clone());

    // 测试插入知识节点 SQL 语法正确
    let node = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "Rust 内存安全".to_string(),
        node_description: "Rust 的内存安全特性".to_string(),
        node_type: "concept".to_string(),
        summary: "Rust 通过所有权系统实现内存安全".to_string(),
        status: MemoryStatus::Active,
        created_at: 0,
        updated_at: 0,
    };

    let result = dao.save_knowledge_node(ctx.clone(), &node).await;
    assert!(result.is_ok());

    // 查询验证插入成功
    let dao = MemoryDaoSqliteImpl::new();
    let ctx2 = RequestContext::new_simple("test-user", pool);
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
    let ctx = RequestContext::new_simple("test-user", pool);

    // 先创建两个节点
    let node1 = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "Rust".to_string(),
        node_description: "Rust 编程语言".to_string(),
        node_type: "language".to_string(),
        summary: "Rust 是一门系统编程语言".to_string(),
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
    let relations = dao.list_outgoing_relations(ctx.clone(), "node-1").await.unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].source_node_id, "node-1");
    assert_eq!(relations[0].target_node_id, "node-2");
}

#[sqlx::test]
async fn test_add_knowledge_reference(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = RequestContext::new_simple("test-user", pool);

    // 先创建节点
    let node = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "测试节点".to_string(),
        node_description: "测试描述".to_string(),
        node_type: "test".to_string(),
        summary: "测试摘要".to_string(),
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
    let references = dao.list_knowledge_references(ctx.clone(), "node-1").await.unwrap();
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
    let ctx = RequestContext::new_simple("test-user", pool);

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
    let ctx = RequestContext::new_simple("test-user", pool.clone());

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
    dao.create_short_term_index(ctx.clone(), index).await.unwrap();

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

    dao.update_short_term_index(ctx.clone(), updated_index).await.unwrap();

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
    let ctx = RequestContext::new_simple("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();

    // 创建多个短期记忆
    for i in 0..5 {
        let index = ShortTermMemoryIndexPo {
            id: format!("st-{}", i),
            agent_id: if i < 3 { "agent-1".to_string() } else { "agent-2".to_string() },
            task_id: None,
            role: "user".to_string(),
            summary: format!("摘要 {}", i),
            tags: serde_json::to_string(&vec!["test"]).unwrap(),
            trace_ids: serde_json::to_string(&vec![format!("trace-{}", i)]).unwrap(),
            status: if i == 0 { MemoryStatus::Forgotten } else { MemoryStatus::Active },
            created_at: now,
            updated_at: now,
        };
        dao.create_short_term_index(ctx.clone(), index).await.unwrap();
    }

    // 测试 list_short_term_by_agent
    let list = dao.list_short_term_by_agent(ctx.clone(), "agent-1", 10).await.unwrap();
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
    let ctx = RequestContext::new_simple("test-user", pool.clone());

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
    dao.create_short_term_index(ctx.clone(), index).await.unwrap();

    // 遗忘（软删除）
    dao.forget_short_term_index(ctx.clone(), "st-forget").await.unwrap();

    // 验证无法再通过 get_short_term_index 获取（软删除过滤）
    let fetched = dao.get_short_term_index(ctx.clone(), "st-forget").await.unwrap();
    assert!(fetched.is_none(), "软删除后无法通过 get_short_term_index 获取");

    // 验证可以通过 query_short_term 获取（设置 exclude_status=Some(MemoryStatus::Active) 只排除 Active，不排除 Forgotten）
    use crate::service::dao::memory::MemoryQuery;
    let query = MemoryQuery {
        ids: Some(vec!["st-forget".to_string()]),
        exclude_status: Some(MemoryStatus::Active),
        ..Default::default()
    };
    let results = dao.query_short_term(ctx, query).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].status, MemoryStatus::Forgotten);}


#[sqlx::test]
async fn test_update_and_list_knowledge_nodes(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = RequestContext::new_simple("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    let node = LongTermKnowledgeNodePo {
        id: "node-update".to_string(),
        agent_id: "test-agent".to_string(),
        node_name: "测试节点".to_string(),
        node_description: "原始描述".to_string(),
        node_type: "concept".to_string(),
        summary: "原始摘要".to_string(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    dao.save_knowledge_node(ctx.clone(), &node).await.unwrap();

    // 测试 update_knowledge_node
    let mut updated_node = node.clone();
    updated_node.summary = "更新后的摘要".to_string();
    dao.update_knowledge_node(ctx.clone(), &updated_node).await.unwrap();

    let fetched = dao.get_knowledge_node(ctx.clone(), "node-update").await.unwrap();
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
            status: MemoryStatus::Active,
            created_at: now,
            updated_at: now,
        },
    ];
    dao.batch_save_knowledge_nodes(ctx.clone(), &nodes).await.unwrap();

    // 测试 list_knowledge_nodes_by_agent
    let list = dao.list_knowledge_nodes_by_agent(ctx.clone(), "test-agent", None, 10).await.unwrap();
    assert_eq!(list.len(), 3); // node-update, node-batch-1, node-batch-2
}

#[sqlx::test]
async fn test_query_and_delete_knowledge_nodes(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = RequestContext::new_simple("test-user", pool.clone());

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
            status: MemoryStatus::Active,
            created_at: now,
            updated_at: now,
        },
    ];
    dao.batch_save_knowledge_nodes(ctx.clone(), &nodes).await.unwrap();

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
    use crate::service::dao::memory::MemorySearch;
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
    dao.delete_knowledge_node(ctx.clone(), "query-1").await.unwrap();
    let deleted = dao.get_knowledge_node(ctx.clone(), "query-1").await.unwrap();
    assert!(deleted.is_none());
}

#[sqlx::test]
async fn test_knowledge_relations(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = RequestContext::new_simple("test-user", pool.clone());

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
            status: MemoryStatus::Active,
            created_at: now,
            updated_at: now,
        },
    ];
    dao.batch_save_knowledge_nodes(ctx.clone(), &nodes).await.unwrap();

    // 测试 batch_add_knowledge_relations
    let relations = vec![
        KnowledgeNodeRelationPo {
            id: "rel-rel-1-2".to_string(),
            source_node_id: "rel-1".to_string(),
            target_node_id: "rel-2".to_string(),
            relation_type: KnowledgeRelationType::Related,
            created_at: now,
            updated_at: now,
        },
    ];
    dao.batch_add_knowledge_relations(ctx.clone(), &relations).await.unwrap();

    // 测试 list_outgoing_relations
    let outgoing = dao.list_outgoing_relations(ctx.clone(), "rel-1").await.unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].target_node_id, "rel-2");

    // 测试 list_incoming_relations
    let incoming = dao.list_incoming_relations(ctx.clone(), "rel-2").await.unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].source_node_id, "rel-1");

    // 测试 delete_knowledge_relation (按 relation_id)
    dao.delete_knowledge_relation(ctx.clone(), "rel-rel-1-2").await.unwrap();
    let outgoing_after = dao.list_outgoing_relations(ctx.clone(), "rel-1").await.unwrap();
    assert_eq!(outgoing_after.len(), 0);
}

#[sqlx::test]
async fn test_knowledge_references(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = RequestContext::new_simple("test-user", pool.clone());

    let now = chrono::Utc::now().timestamp();
    // 创建节点
    let node = LongTermKnowledgeNodePo {
        id: "ref-node".to_string(),
        agent_id: "test-agent".to_string(),
        node_name: "引用测试节点".to_string(),
        node_description: "引用测试描述".to_string(),
        node_type: "test".to_string(),
        summary: "引用测试摘要".to_string(),
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
    dao.batch_add_knowledge_references(ctx.clone(), &references).await.unwrap();

    // 测试 list_knowledge_references
    let refs = dao.list_knowledge_references(ctx.clone(), "ref-node").await.unwrap();
    assert_eq!(refs.len(), 2);
}
