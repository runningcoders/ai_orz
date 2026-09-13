//! Message Vector DAO 单元测试
//! 使用 InMemoryVectorStore（纯 Rust 实现，零系统依赖）

use crate::models::vector::{VectorIndexParams, VectorPayload};
use crate::pkg::RequestContext;
use crate::service::dao::message::{self, MessageQuery, MessageVectorDao};
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn MessageVectorDao> {
    message::new_message_vector_dao()
}

/// 创建测试向量参数
fn create_test_vector_params(message_id: &str, dimension: usize) -> VectorIndexParams {
    VectorIndexParams {
        vector: (0..dimension)
            .map(|i| i as f32 / dimension as f32)
            .collect(),
        content_hash: format!("hash_{}", message_id),
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
        let message_id = format!("msg_{}", i);
        let mut params = create_test_vector_params(&message_id, 3);
        params.vector = vec![i as f32 * 0.1, i as f32 * 0.2, i as f32 * 0.3];
        vector_dao
            .upsert_vector(ctx.clone(), &message_id, &params)
            .await?;
    }

    // 搜索最接近 msg_0 的向量
    let query_vector = vec![0.0, 0.0, 0.0];
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 2, &MessageQuery::default())
        .await?;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].row.id, "msg_0");

    Ok(())
}

/// 测试 upsert 更新已有向量
#[sqlx::test]
async fn test_upsert_update_existing(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let message_id = "msg_0";

    let mut params1 = create_test_vector_params(message_id, 3);
    params1.vector = vec![1.0, 0.0, 0.0];
    vector_dao
        .upsert_vector(ctx.clone(), message_id, &params1)
        .await?;

    let mut params2 = create_test_vector_params(message_id, 3);
    params2.vector = vec![0.0, 1.0, 0.0];
    vector_dao
        .upsert_vector(ctx.clone(), message_id, &params2)
        .await?;

    let query_vector = vec![0.0, 1.0, 0.0];
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 1, &MessageQuery::default())
        .await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, message_id);

    Ok(())
}

/// 测试删除向量索引
#[sqlx::test]
async fn test_delete_vector(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let message_id = "msg_to_delete";
    let params = create_test_vector_params(message_id, 3);
    vector_dao
        .upsert_vector(ctx.clone(), message_id, &params)
        .await?;

    // 确认已存在
    let row = vector_dao.get_vector_row(ctx.clone(), message_id).await?;
    assert!(row.is_some());

    // 删除
    vector_dao.delete_vector(ctx.clone(), message_id).await?;

    // 确认已删除
    let row = vector_dao.get_vector_row(ctx.clone(), message_id).await?;
    assert!(row.is_none());

    Ok(())
}

/// 测试空集合搜索返回空
#[sqlx::test]
async fn test_search_vector_empty(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    let results = vector_dao
        .search_vector(ctx.clone(), &[0.0, 0.0, 0.0], 10, &MessageQuery::default())
        .await?;

    assert_eq!(results.len(), 0);

    Ok(())
}

/// 测试带业务过滤的向量搜索（pre-filter 谓词下推：Top-K 在满足谓词的候选集内选取）
#[sqlx::test]
async fn test_search_vector_with_filter(pool: SqlitePool) -> Result<()> {
    let ctx = new_ctx("test_user", pool.clone());
    let vector_dao = init_test_env();

    // msg_other：全局最近（to_id 为另一个接收方）
    let mut params_other = create_test_vector_params("msg_other", 3);
    params_other.vector = vec![1.0, 0.0, 0.0];
    params_other.payload = VectorPayload {
        to_id: Some("agent_other".to_string()),
        ..Default::default()
    };
    params_other.payload_hash = params_other.payload.hash();
    vector_dao
        .upsert_vector(ctx.clone(), "msg_other", &params_other)
        .await?;

    // msg_own：稍远（to_id 为目标接收方）
    let mut params_own = create_test_vector_params("msg_own", 3);
    params_own.vector = vec![0.6, 0.8, 0.0];
    params_own.payload = VectorPayload {
        to_id: Some("agent_1".to_string()),
        ..Default::default()
    };
    params_own.payload_hash = params_own.payload.hash();
    vector_dao
        .upsert_vector(ctx.clone(), "msg_own", &params_own)
        .await?;

    // 基线：无 filter 时全局最近是 msg_other
    let query_vector = vec![1.0, 0.0, 0.0];
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 1, &MessageQuery::default())
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, "msg_other");

    // pre-filter：to_id=agent_1 时 top_k=1 必须命中 msg_own（post-filter 实现会返回空）
    let filters = MessageQuery {
        to_id: Some("agent_1".to_string()),
        ..Default::default()
    };
    let results = vector_dao
        .search_vector(ctx.clone(), &query_vector, 1, &filters)
        .await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row.id, "msg_own");
    assert_eq!(results[0].row.payload.to_id.as_deref(), Some("agent_1"));

    Ok(())
}
