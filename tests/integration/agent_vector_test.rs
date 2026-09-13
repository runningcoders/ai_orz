//! Agent 向量搜索 pre-filter 谓词下推集成测试
//!
//! 覆盖：
//! - 软删除排除（exclude_status 补集 Any 下推）：Top-K 不被软删除 Agent 的向量行稀释
//! - 正向 status 下推
//!
//! 模式与 memory / message 域一致：手工向量直写 vector store（绕开真实 embedding），
//! 走 DAO 搜索路径断言转译语义。

#[path = "../common/mod.rs"]
mod common;

use ::common::enums::AgentStatus;
use ai_orz::models::vector::{VectorIndexParams, VectorPayload};
use ai_orz::service::dao::agent::{AgentQuery, new_agent_vector_dao};
use sqlx::SqlitePool;

/// 手工构造向量索引参数（绕开真实 embedding，直写 vector store）
fn manual_vector_params(vector: Vec<f32>, payload: VectorPayload) -> VectorIndexParams {
    let payload_hash = payload.hash();
    VectorIndexParams {
        vector,
        content_hash: sha256::digest(uuid::Uuid::now_v7().to_string()),
        payload,
        payload_hash,
        model_provider_id: "manual-test-provider".to_string(),
        embedding_model: "manual-test-model".to_string(),
        expire_at: None,
    }
}

/// pre-filter 谓词下推：软删除 Agent 不稀释 Top-K
///
/// 数据布局（query=[1,0]，余弦距离）：
/// - 软删除（Deleted）Agent 全局最近（distance=0）：若为 post-filter（先取 Top-K 再过滤），
///   exclude_status=Deleted 视角 top_k=1 会取到它再被过滤掉 → 返回空；
///   正确的 pre-filter 应在满足谓词的候选集内选取 → 命中活跃 Agent。
#[sqlx::test]
async fn test_agent_vector_search_prefilter_status(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;

    let deleted_id = format!("ag_pf_{}", uuid::Uuid::now_v7());
    let active_id = format!("ag_pf_{}", uuid::Uuid::now_v7());

    let store = ctx.vector_store();
    store
        .upsert(
            "agents",
            &deleted_id,
            &manual_vector_params(
                vec![1.0, 0.0],
                VectorPayload {
                    status: Some(AgentStatus::Deleted.to_i32().to_string()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert deleted agent failed");
    store
        .upsert(
            "agents",
            &active_id,
            &manual_vector_params(
                vec![0.6, 0.8],
                VectorPayload {
                    status: Some(AgentStatus::Onboarded.to_i32().to_string()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("upsert active agent failed");

    let dao = new_agent_vector_dao();
    let query_vector: Vec<f32> = vec![1.0, 0.0];

    // 基线：无 filter 时全局最近的是软删除 Agent（证明数据已落库且距离排序生效）
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &AgentQuery::default())
        .await
        .expect("search without filter failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].row.id, deleted_id,
        "无 filter 时全局最近应为软删除 Agent"
    );

    // exclude_status=Deleted（search() 默认注入）：top_k=1 必须命中活跃 Agent
    let exclude_query = AgentQuery {
        exclude_status: Some(AgentStatus::Deleted),
        ..Default::default()
    };
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &exclude_query)
        .await
        .expect("search with exclude_status failed");
    assert_eq!(
        hits.len(),
        1,
        "pre-filter 后 top-1 必须命中（post-filter 实现会返回空）"
    );
    assert_eq!(hits[0].row.id, active_id);
    assert_eq!(
        hits[0].row.payload.status.as_deref(),
        Some(AgentStatus::Onboarded.to_i32().to_string().as_str())
    );

    // 放大 top_k：软删除 Agent 不应出现
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 5, &exclude_query)
        .await
        .expect("search with exclude_status (top 5) failed");
    assert!(!hits.is_empty());
    assert!(
        !hits.iter().any(|h| h.row.id == deleted_id),
        "软删除排除：Deleted Agent 不应出现"
    );

    // 正向 status=Onboarded 下推：命中活跃 Agent
    let status_query = AgentQuery {
        status: Some(AgentStatus::Onboarded),
        ..Default::default()
    };
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &status_query)
        .await
        .expect("search with status failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].row.id, active_id);

    // 正向 status=Deleted 下推：命中软删除 Agent（等值语义，与 exclude 互补验证）
    let status_query = AgentQuery {
        status: Some(AgentStatus::Deleted),
        ..Default::default()
    };
    let hits = dao
        .search_vector(ctx.clone(), &query_vector, 1, &status_query)
        .await
        .expect("search with deleted status failed");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].row.id, deleted_id);
}
