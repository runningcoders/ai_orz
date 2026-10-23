//! Memory DAO 短期记忆（short_term）单元测试：索引 CRUD、组合查询、FTS5 搜索与排序

use crate::models::memory::ShortTermMemoryIndexPo;
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use common::enums::MemoryStatus;
use sqlx::SqlitePool;

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
