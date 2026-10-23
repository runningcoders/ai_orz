//! Memory DAO FTS5 触发器同步单元测试：短期记忆与知识节点 FTS 表的增删改同步

use crate::models::memory::{LongTermKnowledgeNodePo, ShortTermMemoryIndexPo};
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use common::enums::MemoryStatus;
use sqlx::{Row, SqlitePool};

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
