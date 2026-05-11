//! Skill Vector DAO SQLite 单元测试

use sqlx::SqlitePool;
use crate::error::AppError;
use crate::models::vector::VectorIndexParams;
use crate::pkg::RequestContext;
use crate::pkg::storage::SqliteVssStore;
use crate::service::dao::skill::{self, SkillVectorDao};
use std::sync::Arc;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 初始化测试依赖
fn init_test() {
    // 必须先初始化 config
    let _ = crate::config::init();
}

/// 初始化向量表（测试专用）
async fn init_vector_collection(pool: SqlitePool) -> Result<(), AppError> {
    let store = SqliteVssStore::from_pool(pool);
    // skills 集合使用 3 维向量（测试用）
    store.create_collection("skills", 3).await?;
    Ok(())
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn SkillVectorDao> {
    init_test();
    skill::new_skill_vector_dao()
}

/// 创建测试向量参数
fn create_test_vector_params(skill_id: &str, dimension: usize) -> VectorIndexParams {
    VectorIndexParams {
        vector: (0..dimension).map(|i| i as f32 / dimension as f32).collect(),
        content_hash: format!("hash_{}", skill_id),
        model_provider_id: "test_provider".to_string(),
        embedding_model: "test-embedding-v1".to_string(),
        expire_at: None,
    }
}

/// 测试插入向量索引并搜索
#[sqlx::test]
async fn test_upsert_and_search_vector(pool: SqlitePool) -> Result<(), AppError> {
    init_vector_collection(pool.clone()).await?;
    let vector_dao = init_test_env();

    // 插入 3 个技能向量
    let skill_ids = vec!["skill_1", "skill_2", "skill_3"];
    for (i, skill_id) in skill_ids.iter().enumerate() {
        let mut vector_params = create_test_vector_params(skill_id, 3);
        // 让向量有明显差异，便于测试搜索结果排序
        vector_params.vector = vec![i as f32 * 0.1, i as f32 * 0.1, i as f32 * 0.1];

        let ctx = new_ctx("test-user", pool.clone());
        vector_dao.upsert_vector(ctx, skill_id, &vector_params).await?;
    }

    // 搜索：用 skill_1 的向量搜索，应该最匹配 skill_1
    let query_vector = vec![0.0, 0.0, 0.0]; // 与 skill_1 完全匹配
    let ctx = new_ctx("test-user", pool.clone());
    let results = vector_dao.search_vector(ctx, &query_vector, 5).await?;

    assert_eq!(results.len(), 3, "应该返回 3 个结果");
    assert_eq!(results[0].0, "skill_1", "第一个结果应该是最匹配的 skill_1");
    assert!(results[0].1 < 0.001, "skill_1 距离应该接近 0");

    Ok(())
}

/// 测试更新向量索引（相同 skill_id 覆盖）
#[sqlx::test]
async fn test_upsert_update_existing(pool: SqlitePool) -> Result<(), AppError> {
    init_vector_collection(pool.clone()).await?;
    let vector_dao = init_test_env();
    let skill_id = "skill_update_test";

    // 第一次插入
    let vector_params_v1 = create_test_vector_params(skill_id, 3);
    let ctx = new_ctx("test-user", pool.clone());
    vector_dao.upsert_vector(ctx, skill_id, &vector_params_v1).await?;

    // 第二次插入（更新）：不同的向量，不同的 hash
    let mut vector_params_v2 = vector_params_v1.clone();
    vector_params_v2.vector = vec![1.0, 1.0, 1.0];
    vector_params_v2.content_hash = "hash_updated".to_string();

    let ctx = new_ctx("test-user", pool.clone());
    vector_dao.upsert_vector(ctx, skill_id, &vector_params_v2).await?;

    // 验证：搜索应该返回更新后的距离
    let ctx = new_ctx("test-user", pool.clone());
    let results = vector_dao.search_vector(ctx, &vec![1.0, 1.0, 1.0], 1).await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, skill_id);
    assert!(results[0].1 < 0.001, "更新后向量应该完全匹配");

    Ok(())
}

/// 测试获取 content_hash
#[sqlx::test]
async fn test_get_content_hash(pool: SqlitePool) -> Result<(), AppError> {
    init_vector_collection(pool.clone()).await?;
    let vector_dao = init_test_env();
    let skill_id = "skill_hash_test";
    let expected_hash = "test_hash_12345";

    // 插入前应该是 None
    let ctx = new_ctx("test-user", pool.clone());
    let hash_before = vector_dao.get_content_hash(ctx, skill_id).await?;
    assert!(hash_before.is_none(), "插入前应该没有 hash");

    // 插入向量
    let mut vector_params = create_test_vector_params(skill_id, 3);
    vector_params.content_hash = expected_hash.to_string();

    let ctx = new_ctx("test-user", pool.clone());
    vector_dao.upsert_vector(ctx, skill_id, &vector_params).await?;

    // 插入后应该能获取到 hash
    let ctx = new_ctx("test-user", pool.clone());
    let hash_after = vector_dao.get_content_hash(ctx, skill_id).await?;
    assert!(hash_after.is_some(), "插入后应该能获取到 hash");
    assert_eq!(hash_after.unwrap(), expected_hash, "hash 应该匹配");

    Ok(())
}

/// 测试获取不存在的 skill 的 content_hash
#[sqlx::test]
async fn test_get_content_hash_not_found(pool: SqlitePool) -> Result<(), AppError> {
    init_vector_collection(pool.clone()).await?;
    let vector_dao = init_test_env();

    let ctx = new_ctx("test-user", pool.clone());
    let hash = vector_dao.get_content_hash(ctx, "non_existent_skill").await?;
    assert!(hash.is_none(), "不存在的 skill 应该返回 None");

    Ok(())
}

/// 测试搜索结果数量限制（top_k）
#[sqlx::test]
async fn test_search_vector_top_k_limit(pool: SqlitePool) -> Result<(), AppError> {
    init_vector_collection(pool.clone()).await?;
    let vector_dao = init_test_env();

    // 插入 5 个技能向量
    for i in 0..5 {
        let skill_id = format!("skill_{}", i);
        let vector_params = VectorIndexParams {
            vector: vec![i as f32 * 0.2; 3],
            content_hash: format!("hash_{}", i),
            model_provider_id: "test".to_string(),
            embedding_model: "test-model".to_string(),
            expire_at: None,
        };

        let ctx = new_ctx("test-user", pool.clone());
        vector_dao.upsert_vector(ctx, &skill_id, &vector_params).await?;
    }

    // 只取前 2 个结果
    let ctx = new_ctx("test-user", pool.clone());
    let results = vector_dao.search_vector(ctx, &vec![0.0, 0.0, 0.0], 2).await?;

    assert_eq!(results.len(), 2, "应该只返回 top 2 结果");

    Ok(())
}

/// 测试空向量库搜索
#[sqlx::test]
async fn test_search_vector_empty(pool: SqlitePool) -> Result<(), AppError> {
    init_vector_collection(pool.clone()).await?;
    let vector_dao = init_test_env();

    let ctx = new_ctx("test-user", pool.clone());
    let results = vector_dao.search_vector(ctx, &vec![0.0, 0.0, 0.0], 10).await?;

    assert!(results.is_empty(), "空向量库应该返回空结果");

    Ok(())
}
