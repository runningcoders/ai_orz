//! Model Provider DAL 单元测试

use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use crate::pkg::RequestContext;
use crate::service::dal::model_provider::ModelProviderDal;
use common::enums::{ModelCapability, ModelProviderStatus, ProviderType};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// 初始化测试环境
async fn init_test_env(
    pool: SqlitePool,
) -> (Arc<dyn ModelProviderDal + Send + Sync>, RequestContext) {
    crate::service::dao::model_provider::init();
    crate::service::dal::model_provider::init();
    let dal = crate::service::dal::model_provider::dal();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (dal, ctx)
}

/// 创建测试 ModelProvider
fn create_test_provider(
    name: &str,
    provider_type: ProviderType,
    model_name: &str,
) -> ModelProvider {
    ModelProvider::new(
        name.to_string(),
        provider_type,
        ModelCapability::Agent,
        model_name.to_string(),
        "test-key".to_string(),
        None,
        None,
        "admin".to_string(),
    )
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let provider = ModelProvider::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        ModelCapability::Agent,
        "gpt-4o".to_string(),
        "sk-xxx".to_string(),
        None,
        Some("OpenAI GPT-4o 官方模型".to_string()),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &provider).await.unwrap();
    let found = dal.find_by_id(ctx, &provider.po.id).await.unwrap().unwrap();

    assert_eq!(found.po.name, "OpenAI GPT-4o".to_string());
    assert_eq!(found.po.provider_type, ProviderType::OpenAI);
    assert_eq!(found.po.model_name, "gpt-4o".to_string());
    assert_eq!(found.po.created_by, "admin".to_string());
}

#[sqlx::test]
async fn test_find_all(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let providers = vec![
        ("OpenAI", ProviderType::OpenAI, "gpt-4o"),
        ("DeepSeek", ProviderType::DeepSeek, "deepseek-chat"),
        ("Ollama", ProviderType::Ollama, "llama3"),
    ];

    for (name, ptype, model) in providers {
        let provider = create_test_provider(name, ptype, model);
        dal.create(ctx.clone(), &provider).await.unwrap();
    }

    let all = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 3);
}

#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool.clone()).await;

    let provider = create_test_provider("Original", ProviderType::OpenAI, "gpt-4");
    dal.create(ctx.clone(), &provider).await.unwrap();

    let mut updated = provider.clone();
    updated.po.name = "Updated".to_string();
    updated.po.model_name = "gpt-4o".to_string();
    updated.touch("editor");

    dal.update(crate::pkg::request_context_test_support::new_test_ctx("editor", pool), &updated)
        .await
        .unwrap();

    let found = dal.find_by_id(ctx, &updated.po.id).await.unwrap().unwrap();
    assert_eq!(found.po.name, "Updated".to_string());
    assert_eq!(found.po.model_name, "gpt-4o".to_string());
    assert_eq!(found.po.modified_by, "editor".to_string());
}

#[sqlx::test]
async fn test_delete(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let provider = create_test_provider("ToDelete", ProviderType::OpenAI, "gpt-4o");
    dal.create(ctx.clone(), &provider).await.unwrap();

    dal.delete(ctx.clone(), &provider).await.unwrap();
    assert!(
        dal.find_by_id(ctx, &provider.po.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    assert!(dal.find_by_id(ctx, "not-exists").await.unwrap().is_none());
}

#[sqlx::test]
async fn test_create_with_custom_base_url(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let provider = ModelProvider::new(
        "Custom OpenAI Compatible".to_string(),
        ProviderType::Custom,
        ModelCapability::Agent,
        "custom-model".to_string(),
        "custom-key".to_string(),
        Some("https://custom.api.com/v1".to_string()),
        Some("自定义兼容接口".to_string()),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &provider).await.unwrap();
    let found = dal.find_by_id(ctx, &provider.po.id).await.unwrap().unwrap();

    assert_eq!(
        found.po.base_url,
        Some("https://custom.api.com/v1".to_string())
    );
    assert_eq!(found.po.provider_type, ProviderType::Custom);
}

#[sqlx::test]
async fn test_all_provider_types(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let cases = vec![
        (ProviderType::OpenAI, "OpenAI"),
        (ProviderType::Custom, "Custom"),
        (ProviderType::DeepSeek, "DeepSeek"),
        (ProviderType::Doubao, "Doubao"),
        (ProviderType::Qwen, "Qwen"),
        (ProviderType::Ollama, "Ollama"),
    ];

    for (ptype, name) in cases {
        let provider = create_test_provider(name, ptype, "model");
        dal.create(ctx.clone(), &provider).await.unwrap();
    }

    let all = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 6);
}
