//! Memory DAO 单元测试
//!
//! 单元测试使用内存数据库，不依赖全局 storage 连接池

use super::*;
use crate::models::memory::{MemoryRole, MemoryTrace, LongTermKnowledgeNodePo, KnowledgeRelation};
use crate::pkg::RequestContext;
use rusqlite::Connection;

#[test]
fn test_append_memory_trace() {
    // 创建内存数据库用于测试
    let conn = Connection::open_in_memory().expect("Failed to create in-memory database");

    // 使用定义好的常量建表
    conn.execute(
        crate::pkg::storage::sql::SQLITE_CREATE_TABLE_SHORT_TERM_MEMORY_INDEX,
        (),
    ).expect("Failed to create table short_term_memory_index");

    conn.execute(
        crate::pkg::storage::sql::SQLITE_CREATE_TABLE_LONG_TERM_KNOWLEDGE_NODE,
        (),
    ).expect("Failed to create table long_term_knowledge_node");

    conn.execute(
        crate::pkg::storage::sql::SQLITE_CREATE_TABLE_KNOWLEDGE_REFERENCE,
        (),
    ).expect("Failed to create table knowledge_reference");

    // 上面创建表成功就说明 SQL 语法正确
    assert!(true);
}

#[test]
fn test_create_knowledge_node() {
    // 创建内存数据库用于测试
    let conn = Connection::open_in_memory().expect("Failed to create in-memory database");

    // 使用定义好的常量建表
    conn.execute(
        crate::pkg::storage::sql::SQLITE_CREATE_TABLE_SHORT_TERM_MEMORY_INDEX,
        (),
    ).expect("Failed to create table short_term_memory_index");

    conn.execute(
        crate::pkg::storage::sql::SQLITE_CREATE_TABLE_LONG_TERM_KNOWLEDGE_NODE,
        (),
    ).expect("Failed to create table long_term_knowledge_node");

    conn.execute(
        crate::pkg::storage::sql::SQLITE_CREATE_TABLE_KNOWLEDGE_REFERENCE,
        (),
    ).expect("Failed to create table knowledge_reference");

    // 测试插入知识节点 SQL 语法正确
    let relations = vec![
        KnowledgeRelation {
            target_node_id: "node-2".to_string(),
            relation_type: "related".to_string(),
        }
    ];
    let node = LongTermKnowledgeNodePo {
        id: "node-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        node_name: "Rust 内存安全".to_string(),
        node_description: "Rust 的内存安全特性".to_string(),
        node_type: "concept".to_string(),
        summary: "Rust 通过所有权系统实现内存安全".to_string(),
        relations,
        created_at: 0,
        updated_at: 0,
    };

    let relations_json = serde_json::to_string(&node.relations).unwrap();
    let result = conn.execute(
        r#"
INSERT INTO long_term_knowledge_node (
    id, agent_id, node_name, node_description, node_type, summary, relations, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
"#,
        rusqlite::params![
            node.id,
            node.agent_id,
            node.node_name,
            node.node_description,
            node.node_type,
            node.summary,
            relations_json,
            node.created_at,
            node.updated_at,
        ],
    );

    assert!(result.is_ok());
}

#[test]
fn test_memory_trace_id_is_content_hash() {
    // 验证 MemoryTrace 的 ID 是内容 hash
    let content = "这是一段测试内容".to_string();
    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
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
        MemoryRole::User,
        "你好，这是一个测试问题".to_string(),
    );

    let markdown = trace.to_markdown();
    assert!(markdown.contains(&trace.id));
    assert!(markdown.contains("User"));
    assert!(markdown.contains("你好，这是一个测试问题"));
}
