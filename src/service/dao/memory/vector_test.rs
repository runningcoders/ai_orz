//! Memory Vector DAO 单元测试
//! 使用 InMemoryVectorStore（纯 Rust 实现，零系统依赖）

use common::error::Error;
use crate::models::vector::VectorIndexParams;
use crate::pkg::RequestContext;
use crate::service::dao::memory::{self, MemoryVectorDao};
use sqlx::SqlitePool;
use std::sync::Arc;
use common::error::Result;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 初始化测试依赖
fn init_test() {
    let _ = crate::config::init();
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn MemoryVectorDao> {
    init_test();
    memory::new_memory_vector_dao()
}

/// 创建测试向量参数
fn create_test_vector_params(id: &str, dimension: usize) -> VectorIndexParams {
    VectorIndexParams {
        vector: (0..dimension)
            .map(|i| i as f32 / dimension as f32)
            .collect(),
        content_hash: format!("hash_{}", id),
        model_provider_id: "test_provider".to_string(),
        embedding_model: "test-embedding-v1".to_string(),
        expire_at: None,
    }
}

// ==================== 短期记忆向量测试 ====================

/// 测试插入短期记忆向量索引并搜索
#[sqlx::test]
async fn test_upsert_and_search_short_term(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    // 插入 3 个短期记忆向量
    for i in 0..3 {
        let memory_id = format!("memory_{}", i);
        let mut params = create_test_vector_params(&memory_id, 3);
        // 让向量有区分度
        params.vector = vec![i as f32 * 0.1, i as f32 * 0.2, i as f32 * 0.3];
        vector_dao
            .upsert_short_term_vector(ctx.clone(), &memory_id, &params)
            .await?;
    }

    // 搜索最接近 memory_0 的向量
    let query_vector = vec![0.0, 0.0, 0.0];
    let results = vector_dao
        .search_short_term_vector(ctx.clone(), &query_vector, 2)
        .await?;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].row.id, "memory_0"); // 第一个应该是最接近的

    Ok(())
}

/// 测试 upsert 可以更新已有短期记忆向量
#[sqlx::test]
async fn test_upsert_update_short_term(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let memory_id = "memory_update";

    // 第一次插入
    let mut params1 = create_test_vector_params(memory_id, 3);
    params1.vector = vec![1.0, 0.0, 0.0];
    vector_dao
        .upsert_short_term_vector(ctx.clone(), memory_id, &params1)
        .await?;

    // 更新向量
    let mut params2 = create_test_vector_params(memory_id, 3);
    params2.vector = vec![0.0, 1.0, 0.0]; // 不同的向量
    vector_dao
        .upsert_short_term_vector(ctx.clone(), memory_id, &params2)
        .await?;

    // 搜索验证用的是更新后的向量
    let query_vector = vec![0.0, 1.0, 0.0];
    let results = vector_dao
        .search_short_term_vector(ctx.clone(), &query_vector, 1)
        .await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, memory_id);
    assert!(results[0].distance < 0.01); // 距离应该非常小

    Ok(())
}

/// 测试获取短期记忆向量行数据
#[sqlx::test]
async fn test_get_short_term_vector_row(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let memory_id = "memory_get";

    // 插入向量
    let params = create_test_vector_params(memory_id, 3);
    vector_dao
        .upsert_short_term_vector(ctx.clone(), memory_id, &params)
        .await?;

    // 获取向量行
    let row = vector_dao
        .get_short_term_vector_row(ctx.clone(), memory_id)
        .await?;

    assert!(row.is_some());
    let row = row.unwrap();
    assert_eq!(row.id, memory_id);
    assert_eq!(row.meta.content_hash, format!("hash_{}", memory_id));

    // 获取不存在的向量
    let not_found = vector_dao
        .get_short_term_vector_row(ctx.clone(), "not_exist")
        .await?;
    assert!(not_found.is_none());

    Ok(())
}

// ==================== 长期知识节点向量测试 ====================

/// 测试插入知识节点向量索引并搜索
#[sqlx::test]
async fn test_upsert_and_search_knowledge_node(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    // 插入 3 个知识节点向量
    for i in 0..3 {
        let knowledge_id = format!("knowledge_{}", i);
        let mut params = create_test_vector_params(&knowledge_id, 3);
        // 让向量有区分度
        params.vector = vec![i as f32 * 0.1, i as f32 * 0.2, i as f32 * 0.3];
        vector_dao
            .upsert_knowledge_node_vector(ctx.clone(), &knowledge_id, &params)
            .await?;
    }

    // 搜索最接近 knowledge_0 的向量
    let query_vector = vec![0.0, 0.0, 0.0];
    let results = vector_dao
        .search_knowledge_node_vector(ctx.clone(), &query_vector, 2)
        .await?;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].row.id, "knowledge_0"); // 第一个应该是最接近的

    Ok(())
}

/// 测试 upsert 可以更新已有知识节点向量
#[sqlx::test]
async fn test_upsert_update_knowledge_node(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let knowledge_id = "knowledge_update";

    // 第一次插入
    let mut params1 = create_test_vector_params(knowledge_id, 3);
    params1.vector = vec![1.0, 0.0, 0.0];
    vector_dao
        .upsert_knowledge_node_vector(ctx.clone(), knowledge_id, &params1)
        .await?;

    // 更新向量
    let mut params2 = create_test_vector_params(knowledge_id, 3);
    params2.vector = vec![0.0, 1.0, 0.0]; // 不同的向量
    vector_dao
        .upsert_knowledge_node_vector(ctx.clone(), knowledge_id, &params2)
        .await?;

    // 搜索验证用的是更新后的向量
    let query_vector = vec![0.0, 1.0, 0.0];
    let results = vector_dao
        .search_knowledge_node_vector(ctx.clone(), &query_vector, 1)
        .await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, knowledge_id);
    assert!(results[0].distance < 0.01); // 距离应该非常小

    Ok(())
}

/// 测试获取知识节点向量行数据
#[sqlx::test]
async fn test_get_knowledge_node_vector_row(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let knowledge_id = "knowledge_get";

    // 插入向量
    let params = create_test_vector_params(knowledge_id, 3);
    vector_dao
        .upsert_knowledge_node_vector(ctx.clone(), knowledge_id, &params)
        .await?;

    // 获取向量行
    let row = vector_dao
        .get_knowledge_node_vector_row(ctx.clone(), knowledge_id)
        .await?;

    assert!(row.is_some());
    let row = row.unwrap();
    assert_eq!(row.id, knowledge_id);
    assert_eq!(row.meta.content_hash, format!("hash_{}", knowledge_id));

    // 获取不存在的向量
    let not_found = vector_dao
        .get_knowledge_node_vector_row(ctx.clone(), "not_exist")
        .await?;
    assert!(not_found.is_none());

    Ok(())
}

// ==================== 命名空间隔离测试 ====================

/// 测试短期记忆和知识节点的向量索引是隔离，互不干扰
#[sqlx::test]
async fn test_namespace_isolation(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    // 在两个 namespace 各插入一个同名 id 的向量，但内容不同
    let common_id = "common_id";

    // 短期记忆向量
    let mut short_params = create_test_vector_params(common_id, 3);
    vector_dao
        .upsert_short_term_vector(ctx.clone(), common_id, &short_params)
        .await?;

    // 知识节点向量（同一个 id，但不同 namespace）
    let mut knowledge_params = create_test_vector_params(common_id, 3);
    vector_dao
        .upsert_knowledge_node_vector(ctx.clone(), common_id, &knowledge_params)
        .await?;

    // 搜索短期记忆，应该只返回短期记忆的结果
    let short_results = vector_dao
        .search_short_term_vector(ctx.clone(), &short_params.vector, 5)
        .await?;
    assert!(!short_results.is_empty());

    // 搜索知识节点，应该只返回知识节点的结果
    let knowledge_results = vector_dao
        .search_knowledge_node_vector(ctx.clone(), &knowledge_params.vector, 5)
        .await?;
    assert!(!knowledge_results.is_empty());

    // 两个 namespace 都能获取到同一个 id 的向量
    let short_row = vector_dao
        .get_short_term_vector_row(ctx.clone(), common_id)
        .await?;
    let knowledge_row = vector_dao
        .get_knowledge_node_vector_row(ctx.clone(), common_id)
        .await?;

    assert!(short_row.is_some());
    assert!(knowledge_row.is_some());

    Ok(())
}
