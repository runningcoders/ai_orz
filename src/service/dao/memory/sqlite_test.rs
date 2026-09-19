//! Memory DAO 单元测试
//!
//! 单元测试使用内存数据库，不依赖全局 storage 连接池

use super::*;
use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgeReferencePo, LongTermKnowledgeNodePo, MemoryTrace,
    ShortTermMemoryIndexPo,
};
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use common::enums::{KnowledgeRelationStatus, MemoryRole, MemoryStatus};
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

    // 批量写入只数一次行 → 位置必须严格连续（第 i 条 = 首行 + i），错一个就等于定位串位
    assert_eq!(positions[1].line_number, positions[0].line_number + 1);
    assert_eq!(positions[2].line_number, positions[0].line_number + 2);

    // 按 (date_filename, line_number) 读回，内容与写入顺序一一对应
    let trace_dir =
        crate::pkg::paths::agent_memory_dir(&crate::config::get().base_data_path(), "test-agent-1");
    let writer = crate::pkg::daily_jsonl::DailyJsonlWriter::new(trace_dir);
    for (position, expected) in positions.iter().zip(traces.iter()) {
        let date = position.date_filename.trim_end_matches(".jsonl");
        let read: MemoryTrace = writer
            .read_line_json(date, position.line_number as usize)
            .expect("position must resolve to a real line");
        assert_eq!(read.id, expected.id);
        assert_eq!(read.input, expected.input);
    }
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

    // 测试 delete_knowledge_relation 只降级生效边：Superseded 历史边不可被覆盖成 Deleted
    dao.delete_knowledge_relation(ctx.clone(), "rel-rel-1-2")
        .await
        .unwrap();
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
        is_published: false,
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

/// 排序方向：待沉淀队列要取「最早未处理」的，否则老记忆会被新记忆持续挤出窗口
#[sqlx::test]
async fn query_short_term_respects_sort_order(pool: SqlitePool) {
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let base = chrono::Utc::now().timestamp_millis();
    // created_at 递增：st-old 最早，st-new 最晚
    for (i, id) in ["st-old", "st-mid", "st-new"].iter().enumerate() {
        let index = ShortTermMemoryIndexPo {
            id: id.to_string(),
            agent_id: "agent-sort".to_string(),
            task_id: None,
            role: "user".to_string(),
            summary: format!("摘要 {}", id),
            tags: "[]".to_string(),
            trace_ids: "[]".to_string(),
            status: MemoryStatus::Active,
            created_at: base + i as i64,
            updated_at: base + i as i64,
        };
        dao.create_short_term_index(ctx.clone(), index)
            .await
            .unwrap();
    }

    use crate::service::dao::memory::{MemoryQuery, MemorySortOrder};

    // 最近优先（默认）：新的在前
    let recent = dao
        .query_short_term(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some("agent-sort".to_string()),
                memory_type: Some(common::enums::MemoryType::ShortTerm),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let recent_ids: Vec<_> = recent.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(recent_ids, vec!["st-new", "st-mid", "st-old"]);

    // 最早优先：队列语义，先进先出
    let oldest = dao
        .query_short_term(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some("agent-sort".to_string()),
                memory_type: Some(common::enums::MemoryType::ShortTerm),
                order: MemorySortOrder::OldestFirst,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let oldest_ids: Vec<_> = oldest.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(oldest_ids, vec!["st-old", "st-mid", "st-new"]);

    // 配合 limit：取最早未处理的 2 条，应拿到最老的两条
    let batch = dao
        .query_short_term(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some("agent-sort".to_string()),
                status: Some(MemoryStatus::Active),
                memory_type: Some(common::enums::MemoryType::ShortTerm),
                limit: Some(2),
                order: MemorySortOrder::OldestFirst,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let batch_ids: Vec<_> = batch.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(batch_ids, vec!["st-old", "st-mid"]);
}

#[test]
fn sort_order_to_sql_is_stable() {
    use crate::service::dao::memory::MemorySortOrder;
    assert_eq!(
        MemorySortOrder::RecentFirst.to_sql(),
        " ORDER BY created_at DESC"
    );
    assert_eq!(
        MemorySortOrder::OldestFirst.to_sql(),
        " ORDER BY created_at ASC"
    );
    // 默认值必须是最近优先，保持历史行为不变
    assert_eq!(MemorySortOrder::default(), MemorySortOrder::RecentFirst);
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
