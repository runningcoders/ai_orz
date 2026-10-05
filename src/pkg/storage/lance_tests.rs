//! prefilter_tests 单元测试（拆分自 lance.rs）
//!
//! 文件瘦身：原 778 行 → 626 行，测试体 153 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::vector::{
    FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorPayload,
};
use crate::pkg::storage::vector::VectorStore;

fn params(agent: &str, published: bool, dim: usize) -> VectorIndexParams {
    let payload = VectorPayload {
        agent_id: Some(agent.to_string()),
        is_published: Some(published),
        ..Default::default()
    };
    VectorIndexParams {
        vector: vec![1.0; dim],
        content_hash: format!("h-{agent}-{published}"),
        payload_hash: payload.hash(),
        payload,
        model_provider_id: "p".into(),
        embedding_model: "m".into(),
        expire_at: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_lance_search_prefilter() {
    let dir = std::env::temp_dir().join(format!("lance_pf_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = LanceVectorStore::new(&dir).unwrap();
    store.init_collection("pf", 4).await.unwrap();
    store
        .upsert("pf", "a1", &params("agent-1", false, 4))
        .await
        .unwrap();
    store
        .upsert("pf", "b1", &params("agent-2", true, 4))
        .await
        .unwrap();
    store
        .upsert("pf", "b2", &params("agent-2", false, 4))
        .await
        .unwrap();

    // 无过滤：3 条
    let all = store
        .search("pf", &[1.0, 1.0, 1.0, 1.0], 10, None)
        .await
        .unwrap();
    assert_eq!(all.len(), 3);

    // agent-1 视角：仅 1 条
    let f = VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()));
    let hits = store
        .search("pf", &[1.0, 1.0, 1.0, 1.0], 10, Some(&f))
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].row.id, "a1");
    assert_eq!(hits[0].row.payload.agent_id.as_deref(), Some("agent-1"));

    // OR 可见性：agent-1 自己的 + 已发布的
    let vis = VectorFilter::Any(vec![
        VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into())),
        VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
    ]);
    let hits = store
        .search("pf", &[1.0, 1.0, 1.0, 1.0], 10, Some(&vis))
        .await
        .unwrap();
    assert_eq!(hits.len(), 2); // a1 + b1
}

#[tokio::test(flavor = "multi_thread")]
async fn test_lance_update_payload() {
    let dir = std::env::temp_dir().join(format!("lance_up_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = LanceVectorStore::new(&dir).unwrap();
    store.init_collection("up", 4).await.unwrap();
    store
        .upsert("up", "a1", &params("agent-1", false, 4))
        .await
        .unwrap();

    let new_p = VectorPayload {
        agent_id: Some("agent-1".into()),
        is_published: Some(true), // 发布翻转场景
        ..Default::default()
    };
    store.update_payload("up", "a1", &new_p).await.unwrap();

    let row = store.get("up", "a1").await.unwrap().unwrap();
    assert_eq!(row.payload.is_published, Some(true));
    assert_eq!(row.meta.payload_hash, new_p.hash());
}

/// 维度自愈：Embedding 维度变化（供应商更换 / dim-0 残废表）后 upsert 应
/// drop 重建而不是被 LanceDB "Append with different schema" 拒绝。
/// 这是全量重建（RebuildVectors）在维度变化场景下能跑通的关键。
#[tokio::test(flavor = "multi_thread")]
async fn test_upsert_dim_mismatch_self_heals() {
    let dir = std::env::temp_dir().join(format!("lance_dim_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = LanceVectorStore::new(&dir).unwrap();

    // 旧维度 4 写入一条
    store.init_collection("dim", 4).await.unwrap();
    store
        .upsert("dim", "old", &params("agent-1", false, 4))
        .await
        .unwrap();

    // 新维度 8 upsert：应自愈（drop 旧表重建），不再报 schema 不一致
    store
        .upsert("dim", "new", &params("agent-2", true, 8))
        .await
        .unwrap();

    // 旧维度行随表重建消失，新维度行可检索
    assert!(store.get("dim", "old").await.unwrap().is_none());
    let hits = store
        .search("dim", &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0], 10, None)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].row.id, "new");
}

/// 只读/删除类访问路径对不存在的集合必须 no-op，不得以 dimensions=0 建出残废表
/// （回归：clear/get/delete/update_payload 曾把缺失集合建成 vector 维度=0 的表，
/// 导致后续正常维度 upsert 全部被 LanceDB 拒绝）。
#[tokio::test(flavor = "multi_thread")]
async fn test_missing_table_ops_do_not_create_table() {
    let dir = std::env::temp_dir().join(format!("lance_no_create_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = LanceVectorStore::new(&dir).unwrap();

    // 全部 dim-0 访问路径打一遍
    assert!(store.get("ghost", "x").await.unwrap().is_none());
    store.delete("ghost", "x").await.unwrap();
    store.clear_collection("ghost").await.unwrap();
    let p = crate::models::vector::VectorPayload::default();
    store.update_payload("ghost", "x", &p).await.unwrap();
    assert!(
        store
            .search("ghost", &[1.0; 4], 10, None)
            .await
            .unwrap()
            .is_empty()
    );

    // 关键断言：没有建出任何表（历史上这里会出现维度=0 的 ghost 表）
    let names = store.db.table_names().execute().await.unwrap();
    assert!(names.is_empty(), "不应创建任何表，实际: {:?}", names);
}
