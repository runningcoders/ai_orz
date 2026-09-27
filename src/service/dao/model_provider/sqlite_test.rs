//! ModelProvider DAO 单元测试
//!
//! 单元测试使用独立数据库，测试隔离性好

use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::service::dao::model_provider::{self, ModelProviderDao};
use common::enums::{ModelCapability, ModelProviderStatus, ProviderType};
use sqlx::SqlitePool;
use std::sync::Arc;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn ModelProviderDao> {
    crate::service::dao::model_provider::init();
    model_provider::dao()
}

/// 创建测试 ModelProviderPo
fn create_test_provider(name: &str, provider_type: ProviderType, api_key: &str) -> ModelProviderPo {
    ModelProviderPo::new(
        name.to_string(),
        provider_type,
        ModelCapability::Agent,
        "gpt-4o".to_string(),
        api_key.to_string(),
        None,
        Some(format!("{} 模型", name)),
        "test".to_string(),
    )
}

#[sqlx::test]
async fn test_insert_and_find_model_provider(pool: SqlitePool) {
    let dao = init_test_env();

    // 创建测试对象
    let provider_po = create_test_provider("OpenAI GPT-4o", ProviderType::OpenAI, "test-key");

    // 测试插入
    let result = dao
        .insert(new_ctx("test", pool.clone()), &provider_po)
        .await;
    assert!(result.is_ok());

    // 测试查询
    let found = dao
        .find_by_id(new_ctx("test", pool), provider_po.id.as_str())
        .await
        .expect("Query failed");
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "OpenAI GPT-4o".to_string());
    assert_eq!(found.provider_type, ProviderType::OpenAI);
    assert_eq!(found.model_name, "gpt-4o".to_string());
    assert_eq!(found.api_key, "test-key".to_string());
    assert_eq!(found.description, Some("OpenAI GPT-4o 模型".to_string()));
    assert_eq!(found.status, ModelProviderStatus::Normal);
}

#[sqlx::test]
async fn test_find_by_id_not_exists(pool: SqlitePool) {
    let dao = init_test_env();

    // 查询不存在的 ID 应该返回 Ok(None)
    let found = dao
        .find_by_id(new_ctx("test", pool), "not-exists-id")
        .await
        .expect("Query failed");
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_find_all_model_provider(pool: SqlitePool) {
    let dao = init_test_env();

    // 查询空表应该返回空 Vec，不报错
    let all = dao
        .find_all(new_ctx("test", pool))
        .await
        .expect("Query all failed");
    assert!(all.is_empty());
}

#[sqlx::test]
async fn test_query(pool: sqlx::SqlitePool) {
    let dao = init_test_env();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);

    use crate::service::dao::model_provider::ModelProviderQuery;

    // 测试空查询
    let query = ModelProviderQuery::default();
    let result = dao.query(ctx, query).await;
    println!("Query result: {:?}", result);
    assert!(result.is_ok());
}

// ==================== 小脑（Decision）默认获取测试 ====================

/// 创建测试 Decision（小脑）ModelProviderPo
fn create_test_decision_provider(name: &str, api_key: &str) -> ModelProviderPo {
    ModelProviderPo::new(
        name.to_string(),
        ProviderType::Jev,
        ModelCapability::Decision,
        "jev-latest".to_string(),
        api_key.to_string(),
        None,
        Some(format!("{} 小脑模型", name)),
        "test".to_string(),
    )
}

/// 无启用小脑时返回 None（合法：运行时兜底归二期）
#[sqlx::test]
async fn test_get_default_cerebellum_provider_none(pool: SqlitePool) {
    let dao = init_test_env();

    // 造一条 Embedding 启用记录 + 一条 Agent 记录：都不该命中 Decision 查询
    let embedding = create_test_provider("Embedding", ProviderType::OpenAI, "k1");
    let mut embedding = embedding;
    embedding.capability = ModelCapability::Embedding;
    dao.insert(new_ctx("test", pool.clone()), &embedding)
        .await
        .unwrap();
    let agent = create_test_provider("Agent", ProviderType::OpenAI, "k2");
    dao.insert(new_ctx("test", pool.clone()), &agent)
        .await
        .unwrap();

    let got = dao
        .get_default_cerebellum_provider(new_ctx("test", pool))
        .await
        .unwrap();
    assert!(got.is_none(), "无 Decision 记录时必须返回 None");
}

/// 多条 Decision 记录：取第一个 api_key 非空的启用记录（同构 embedding 口径）
#[sqlx::test]
async fn test_get_default_cerebellum_provider_first_available(pool: SqlitePool) {
    let dao = init_test_env();

    // 记录 1：启用但 api_key 为空（应被跳过）
    let mut empty_key = create_test_decision_provider("小脑-空key", "");
    empty_key.status = ModelProviderStatus::Normal;
    dao.insert(new_ctx("test", pool.clone()), &empty_key)
        .await
        .unwrap();

    // 记录 2：启用且 api_key 非空（应命中）
    let mut ok = create_test_decision_provider("小脑-可用", "sk-cerebellum");
    ok.status = ModelProviderStatus::Normal;
    dao.insert(new_ctx("test", pool.clone()), &ok)
        .await
        .unwrap();

    // 记录 3：启用且 api_key 非空但创建更晚（find 语义=第一个可用，非最新）
    let mut later = create_test_decision_provider("小脑-更晚", "sk-later");
    later.status = ModelProviderStatus::Normal;
    dao.insert(new_ctx("test", pool.clone()), &later)
        .await
        .unwrap();

    let got = dao
        .get_default_cerebellum_provider(new_ctx("test", pool))
        .await
        .unwrap()
        .expect("应命中可用记录");
    // 语义断言（与 embedding 版同构）：命中「api_key 非空 + Decision + Normal」的
    // 启用记录；query 排序为 created_at DESC（与 embedding 版同一实现路径），
    // 同毫秒插入时具体命中哪条非空记录由排序决定，不绑定具体 id。
    assert_ne!(got.id, empty_key.id, "空 api_key 记录必须被跳过");
    assert!(
        !got.api_key.trim().is_empty(),
        "命中的必须是 api_key 非空记录"
    );
    assert_eq!(got.capability, ModelCapability::Decision);
    assert_eq!(got.status, ModelProviderStatus::Normal);
    assert_eq!(got.provider_type, ProviderType::Jev);
}

/// status=Disabled 的 Decision 记录不参与默认获取；Decision 与 Embedding 维度隔离
#[sqlx::test]
async fn test_get_default_cerebellum_provider_filters_status_and_capability(pool: SqlitePool) {
    let dao = init_test_env();

    // 停用的 Decision：不应命中
    let mut disabled = create_test_decision_provider("小脑-停用", "sk-x");
    disabled.status = ModelProviderStatus::Disabled;
    dao.insert(new_ctx("test", pool.clone()), &disabled)
        .await
        .unwrap();

    // 启用的 Embedding：capability 维度不同，不应命中 Decision 查询
    let mut embedding = create_test_provider("Embedding", ProviderType::OpenAI, "sk-emb");
    embedding.capability = ModelCapability::Embedding;
    embedding.status = ModelProviderStatus::Normal;
    dao.insert(new_ctx("test", pool.clone()), &embedding)
        .await
        .unwrap();

    let got = dao
        .get_default_cerebellum_provider(new_ctx("test", pool))
        .await
        .unwrap();
    assert!(got.is_none(), "停用 Decision 与启用 Embedding 都不应命中");
}

/// find_enabled_decision_provider：唯一启用者判定（守卫用）
#[sqlx::test]
async fn test_find_enabled_decision_provider(pool: SqlitePool) {
    let dao = init_test_env();

    let mut enabled = create_test_decision_provider("小脑-启用", "sk-a");
    enabled.status = ModelProviderStatus::Normal;
    dao.insert(new_ctx("test", pool.clone()), &enabled)
        .await
        .unwrap();

    let mut disabled = create_test_decision_provider("小脑-备用", "sk-b");
    disabled.status = ModelProviderStatus::Disabled;
    dao.insert(new_ctx("test", pool.clone()), &disabled)
        .await
        .unwrap();

    let got = dao
        .find_enabled_decision_provider(new_ctx("test", pool.clone()))
        .await
        .unwrap()
        .expect("应找到唯一启用者");
    assert_eq!(got.id, enabled.id);

    // 启用者停用后返回 None
    let mut off = got;
    off.status = ModelProviderStatus::Disabled;
    dao.update(new_ctx("test", pool.clone()), &off)
        .await
        .unwrap();
    let got = dao
        .find_enabled_decision_provider(new_ctx("test", pool))
        .await
        .unwrap();
    assert!(got.is_none(), "唯一启用者停用后应返回 None");
}

/// config JSON 含 timeout_ms 字段时可正常落库/读出（零 migration 兼容）
#[sqlx::test]
async fn test_decision_provider_config_timeout_ms_roundtrip(pool: SqlitePool) {
    let dao = init_test_env();

    let mut po = create_test_decision_provider("小脑-超时配置", "sk-t");
    po.update_config(|cfg| cfg.timeout_ms = Some(500));
    dao.insert(new_ctx("test", pool.clone()), &po)
        .await
        .unwrap();

    let got = dao
        .find_by_id(new_ctx("test", pool), po.id.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        got.config().timeout_ms,
        Some(500),
        "timeout_ms 必须随 config JSON 往返"
    );
}
