//! Task Vector DAO 单元测试
//! 使用 InMemoryVectorStore（纯 Rust 实现，零系统依赖）

use crate::models::vector::VectorIndexParams;
use crate::pkg::RequestContext;
use crate::service::dao::task::{self, TaskVectorDao};
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试依赖
fn init_test() {
    let _ = crate::config::init();
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn TaskVectorDao> {
    init_test();
    task::new_task_vector_dao()
}

/// 创建测试向量参数
fn create_test_vector_params(task_id: &str, dimension: usize) -> VectorIndexParams {
    VectorIndexParams {
        vector: (0..dimension)
            .map(|i| i as f32 / dimension as f32)
            .collect(),
        content_hash: format!("hash_{}", task_id),
        model_provider_id: "test_provider".to_string(),
        embedding_model: "test-embedding-v1".to_string(),
        expire_at: None,
    }
}

/// 测试插入向量索引并搜索
#[sqlx::test]
async fn test_upsert_and_search_vector(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    // 插入 3 个向量
    for i in 0..3 {
        let task_id = format!("task_{}", i);
        let mut params = create_test_vector_params(&task_id, 3);
        // 让向量有区分度
        params.vector = vec![i as f32 * 0.1, i as f32 * 0.2, i as f32 * 0.3];
        vector_dao
            .upsert_vector(ctx.clone(), &task_id, &params)
            .await?;
    }

    // 搜索最接近 task_0 的向量
    let query_vector = vec![0.0, 0.0, 0.0];
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 2)
        .await?;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].row.id, "task_0"); // 第一个应该是最接近的

    Ok(())
}

/// 测试 upsert 可以更新已有向量
#[sqlx::test]
async fn test_upsert_update_existing(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let task_id = "task_0";

    // 第一次插入
    let mut params1 = create_test_vector_params(task_id, 3);
    params1.vector = vec![1.0, 0.0, 0.0];
    vector_dao
        .upsert_vector(ctx.clone(), task_id, &params1)
        .await?;

    // 更新向量
    let mut params2 = create_test_vector_params(task_id, 3);
    params2.vector = vec![0.0, 1.0, 0.0]; // 不同的向量
    vector_dao
        .upsert_vector(ctx.clone(), task_id, &params2)
        .await?;

    // 搜索验证用的是更新后的向量
    let query_vector = vec![0.0, 1.0, 0.0];
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 1)
        .await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, task_id);
    assert!(results[0].distance < 0.01); // 距离应该非常小

    Ok(())
}

/// 测试获取向量行数据
#[sqlx::test]
async fn test_get_vector_row(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let task_id = "task_get";
    let params = create_test_vector_params(task_id, 3);
    let expected_hash = params.content_hash.clone();
    vector_dao
        .upsert_vector(ctx.clone(), task_id, &params)
        .await?;

    let row = vector_dao.get_vector_row(ctx.clone(), task_id).await?;

    assert!(row.is_some());
    let row = row.unwrap();
    assert_eq!(row.id, task_id);
    assert_eq!(row.meta.content_hash, expected_hash);

    // 获取不存在的向量
    let not_found = vector_dao
        .get_vector_row(ctx.clone(), "non_existent")
        .await?;
    assert!(not_found.is_none());

    Ok(())
}

/// 测试搜索时 top_k 限制生效
#[sqlx::test]
async fn test_search_vector_top_k_limit(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    // 插入 5 个向量
    for i in 0..5 {
        let task_id = format!("task_{}", i);
        let mut params = create_test_vector_params(&task_id, 3);
        params.vector = vec![i as f32 * 0.1, 0.0, 0.0];
        vector_dao
            .upsert_vector(ctx.clone(), &task_id, &params)
            .await?;
    }

    // 只返回 top 2
    let results = vector_dao
        .search_vector(ctx.clone(), &[0.0, 0.0, 0.0], 2)
        .await?;

    assert_eq!(results.len(), 2);

    Ok(())
}

/// 测试空集合搜索返回空
#[sqlx::test]
async fn test_search_vector_empty(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let results = vector_dao
        .search_vector(ctx.clone(), &[0.0, 0.0, 0.0], 10)
        .await?;

    assert_eq!(results.len(), 0);

    Ok(())
}

/// 测试删除向量索引
#[sqlx::test]
async fn test_delete_vector(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let task_id = "task_delete";
    let params = create_test_vector_params(task_id, 3);
    vector_dao
        .upsert_vector(ctx.clone(), task_id, &params)
        .await?;

    // 确认存在
    let row = vector_dao.get_vector_row(ctx.clone(), task_id).await?;
    assert!(row.is_some());

    // 删除
    vector_dao.delete_vector(ctx.clone(), task_id).await?;

    // 确认已删除
    let row = vector_dao.get_vector_row(ctx.clone(), task_id).await?;
    assert!(row.is_none());

    Ok(())
}
