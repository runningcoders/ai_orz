//! Memory DAO 单元测试
//!
//! 单元测试使用内存数据库，不依赖全局 storage 连接池

use super::*;
use crate::models::memory::{MemoryRole, MemoryTrace, LongTermKnowledgeNodePo, KnowledgeNodeRelationPo, KnowledgeReferencePo, KnowledgeRelationType};
use crate::pkg::RequestContext;
use sqlx::SqlitePool;

#[sqlx::test]
async fn test_append_memory_trace(pool: SqlitePool) {
    // 自动迁移已经由 sqlx::test 执行
    let dao = SqliteMemoryDao::new();
    let ctx = RequestContext::new_simple("test-user", pool);

    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        "test-log-1".to_string(),
        "test-user".to_string(),
        "test-org".to_string(),
        MemoryRole::User,
        "这是一段测试内容".to_string(),
    );

    let result = dao.append_memory_trace(
        ctx,
        &trace,
        "测试摘要".to_string(),
        vec!["test".to_string(), "memory".to_string()],
    ).await;

    assert!(result.is_ok());
    let index = result.unwrap();
    assert_eq!(index.agent_id, "test-agent-1");
    assert_eq!(index.summary, "测试摘要");
}

#[sqlx::test]
async fn test_create_knowledge_node(pool: SqlitePool) {
    // 自动迁移已经由 sqlx::test 执行
    let dao = SqliteMemoryDao::new();
    let ctx = RequestContext::new_simple("test-user", pool.clone());

    // 测试插入知识节点 SQL 语法正确
    let node = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "Rust 内存安全".to_string(),
        node_description: "Rust 的内存安全特性".to_string(),
        node_type: "concept".to_string(),
        summary: "Rust 通过所有权系统实现内存安全".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let result = dao.save_knowledge_node(ctx.clone(), &node).await;
    assert!(result.is_ok());

    // 查询验证插入成功
    let dao = SqliteMemoryDao::new();
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
    let dao = SqliteMemoryDao::new();
    let ctx = RequestContext::new_simple("test-user", pool);

    // 先创建两个节点
    let node1 = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "Rust".to_string(),
        node_description: "Rust 编程语言".to_string(),
        node_type: "language".to_string(),
        summary: "Rust 是一门系统编程语言".to_string(),
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
    let dao = SqliteMemoryDao::new();
    let ctx = RequestContext::new_simple("test-user", pool);

    // 先创建节点
    let node = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "测试节点".to_string(),
        node_description: "测试描述".to_string(),
        node_type: "test".to_string(),
        summary: "测试摘要".to_string(),
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
        date_path: "2026-04-11.md".to_string(),
        byte_start: 0,
        byte_length: 100,
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
fn test_memory_trace_id_is_content_hash() {
    // 验证 MemoryTrace 的 ID 是内容 hash
    let content = "这是一段测试内容".to_string();
    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        "log-1".to_string(),
        "user-1".to_string(),
        "org-1".to_string(),
        MemoryRole::User,
        content.clone(),
    );

    let expected_hash = sha256::digest(content.as_bytes());
    assert_eq!(trace.id, expected_hash);
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
    );

    let markdown = trace.to_markdown();
    assert!(markdown.contains(&trace.id));
    assert!(markdown.contains("User"));
    assert!(markdown.contains("你好，这是一个测试问题"));
}
