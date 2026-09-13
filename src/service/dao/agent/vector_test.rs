//! Agent Vector DAO 单元测试
//! 使用 InMemoryVectorStore（纯 Rust 实现，零系统依赖）

use crate::models::vector::{VectorIndexParams, VectorPayload};
use crate::pkg::RequestContext;
use crate::service::dao::agent::{self, AgentQuery, AgentVectorDao};
use common::error::Result;
use sqlx::SqlitePool;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn AgentVectorDao> {
    agent::init_vector();
    agent::new_agent_vector_dao()
}

use std::sync::Arc;

/// 创建测试向量参数
fn create_test_vector_params(agent_id: &str, dimension: usize) -> VectorIndexParams {
    VectorIndexParams {
        vector: (0..dimension)
            .map(|i| i as f32 / dimension as f32)
            .collect(),
        content_hash: format!("hash_{}", agent_id),
        payload: VectorPayload::default(),
        payload_hash: VectorPayload::default().hash(),
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
        let agent_id = format!("agent_{}", i);
        let mut params = create_test_vector_params(&agent_id, 3);
        params.vector = vec![i as f32 * 0.1, i as f32 * 0.2, i as f32 * 0.3];
        vector_dao
            .upsert_vector(ctx.clone(), &agent_id, &params)
            .await?;
    }

    // 搜索最接近 agent_0 的向量
    let query_vector = vec![0.0, 0.0, 0.0];
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 2, &AgentQuery::default())
        .await?;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].row.id, "agent_0");

    Ok(())
}

/// 测试 upsert 可以更新已有向量
#[sqlx::test]
async fn test_upsert_update_existing(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let agent_id = "agent_0";

    let mut params1 = create_test_vector_params(agent_id, 3);
    params1.vector = vec![1.0, 0.0, 0.0];
    vector_dao
        .upsert_vector(ctx.clone(), agent_id, &params1)
        .await?;

    let mut params2 = create_test_vector_params(agent_id, 3);
    params2.vector = vec![0.0, 1.0, 0.0];
    vector_dao
        .upsert_vector(ctx.clone(), agent_id, &params2)
        .await?;

    let query_vector = vec![0.0, 1.0, 0.0];
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 1, &AgentQuery::default())
        .await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, agent_id);

    Ok(())
}

/// 测试获取 content_hash
#[sqlx::test]
async fn test_get_content_hash(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let agent_id = "agent_0";
    let params = create_test_vector_params(agent_id, 3);
    let expected_hash = params.content_hash.clone();
    vector_dao
        .upsert_vector(ctx.clone(), agent_id, &params)
        .await?;

    let row = vector_dao.get_vector_row(ctx.clone(), agent_id).await?;

    assert_eq!(row.map(|r| r.meta.content_hash), Some(expected_hash));

    Ok(())
}

/// 测试获取不存在的 content_hash 返回 None
#[sqlx::test]
async fn test_get_content_hash_not_found(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let row = vector_dao
        .get_vector_row(ctx.clone(), "non_existent")
        .await?;

    assert_eq!(row.map(|r| r.meta.content_hash), None);

    Ok(())
}

/// 测试删除向量
#[sqlx::test]
async fn test_delete_vector(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let agent_id = "agent_0";
    let params = create_test_vector_params(agent_id, 3);
    vector_dao
        .upsert_vector(ctx.clone(), agent_id, &params)
        .await?;

    // 删除
    vector_dao.delete_vector(ctx.clone(), agent_id).await?;

    // 验证已删除
    let row = vector_dao.get_vector_row(ctx.clone(), agent_id).await?;
    assert!(row.is_none());

    Ok(())
}

/// 测试搜索时 top_k 限制生效
#[sqlx::test]
async fn test_search_vector_top_k_limit(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    for i in 0..5 {
        let agent_id = format!("agent_{}", i);
        let mut params = create_test_vector_params(&agent_id, 3);
        params.vector = vec![i as f32 * 0.1, 0.0, 0.0];
        vector_dao
            .upsert_vector(ctx.clone(), &agent_id, &params)
            .await?;
    }

    let results = vector_dao
        .search_vector(ctx.clone(), &[0.0, 0.0, 0.0], 2, &AgentQuery::default())
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
        .search_vector(ctx.clone(), &[0.0, 0.0, 0.0], 10, &AgentQuery::default())
        .await?;

    assert_eq!(results.len(), 0);

    Ok(())
}

/// 测试带业务过滤的向量搜索（pre-filter 谓词下推：Top-K 在满足谓词的候选集内选取）
#[sqlx::test]
async fn test_search_vector_with_filter(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    // agent_deleted：全局最近（status=Deleted，软删除行仍在向量库中）
    let mut params_deleted = create_test_vector_params("agent_deleted", 2);
    params_deleted.vector = vec![1.0, 0.0];
    params_deleted.payload = VectorPayload {
        status: Some(common::enums::AgentStatus::Deleted.to_i32().to_string()),
        ..Default::default()
    };
    params_deleted.payload_hash = params_deleted.payload.hash();
    vector_dao
        .upsert_vector(ctx.clone(), "agent_deleted", &params_deleted)
        .await?;

    // agent_active：稍远（status=Onboarded）
    let mut params_active = create_test_vector_params("agent_active", 2);
    params_active.vector = vec![0.6, 0.8];
    params_active.payload = VectorPayload {
        status: Some(common::enums::AgentStatus::Onboarded.to_i32().to_string()),
        ..Default::default()
    };
    params_active.payload_hash = params_active.payload.hash();
    vector_dao
        .upsert_vector(ctx.clone(), "agent_active", &params_active)
        .await?;

    let query_vector = vec![1.0, 0.0];

    // 基线：无 filter 时全局最近是软删除的 agent
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 1, &AgentQuery::default())
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, "agent_deleted");

    // exclude_status=Deleted（search() 默认注入）：top_k=1 必须命中活跃 agent
    // （post-filter 实现会取到 deleted 行再过滤掉 → 返回空）
    let filters = AgentQuery {
        exclude_status: Some(common::enums::AgentStatus::Deleted),
        ..Default::default()
    };
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 1, &filters)
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, "agent_active");
    assert_eq!(results[0].row.payload.status.as_deref(), Some("3"));

    // status=Onboarded 正向下推：同样命中活跃 agent
    let filters = AgentQuery {
        status: Some(common::enums::AgentStatus::Onboarded),
        ..Default::default()
    };
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 1, &filters)
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, "agent_active");

    // status=Deleted 正向下推：命中软删除 agent
    let filters = AgentQuery {
        status: Some(common::enums::AgentStatus::Deleted),
        ..Default::default()
    };
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 1, &filters)
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, "agent_deleted");

    Ok(())
}
