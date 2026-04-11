//! Model Provider DAL 单元测试

use crate::service::dal::model_provider::{ModelProviderDal, ModelProviderDalTrait};
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use common::enums::{ModelProviderStatus, ProviderType};
use crate::pkg::RequestContext;
use std::sync::Arc;
use sqlx::SqlitePool;
use uuid::Uuid;

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    crate::service::dao::model_provider::init();
    let dal = crate::service::dal::model_provider::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let provider = ModelProvider::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
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
    crate::service::dao::model_provider::init();
    let dal = crate::service::dal::model_provider::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let providers = vec![
        ("OpenAI", ProviderType::OpenAI, "gpt-4o"),
        ("DeepSeek", ProviderType::DeepSeek, "deepseek-chat"),
        ("Ollama", ProviderType::Ollama, "llama3"),
    ];

    for (name, ptype, model) in providers {
        let provider = ModelProvider::new(
            name.to_string(),
            ptype,
            model.to_string(),
            "test-key".to_string(),
            None,
            None,
            "admin".to_string(),
        );
        dal.create(ctx.clone(), &provider).await.unwrap();
    }

    let all = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 3);
}

#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    crate::service::dao::model_provider::init();
    let dal = crate::service::dal::model_provider::dal();
    let ctx = RequestContext::new_simple("admin", pool.clone());

    let provider = ModelProvider::new(
        "Original".to_string(),
        ProviderType::OpenAI,
        "gpt-4".to_string(),
        "key".to_string(),
        None,
        Some("desc".to_string()),
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &provider).await.unwrap();

    let mut updated = provider.clone();
    updated.po.name = "Updated".to_string();
    updated.po.model_name = "gpt-4o".to_string();
    updated.touch("editor");

    dal.update(RequestContext::new_simple("editor", pool), &updated).await.unwrap();

    let found = dal.find_by_id(ctx, &updated.po.id).await.unwrap().unwrap();
    assert_eq!(found.po.name, "Updated".to_string());
    assert_eq!(found.po.model_name, "gpt-4o".to_string());
    assert_eq!(found.po.modified_by, "editor".to_string());
}

#[sqlx::test]
async fn test_delete(pool: SqlitePool) {
    crate::service::dao::model_provider::init();
    let dal = crate::service::dal::model_provider::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let provider = ModelProvider::new(
        "ToDelete".to_string(),
        ProviderType::OpenAI,
        "gpt-4o".to_string(),
        "key".to_string(),
        None,
        None,
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &provider).await.unwrap();

    dal.delete(ctx.clone(), &provider).await.unwrap();
    assert!(dal.find_by_id(ctx, &provider.po.id).await.unwrap().is_none());
}

#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    crate::service::dao::model_provider::init();
    let dal = crate::service::dal::model_provider::dal();
    let ctx = RequestContext::new_simple("user1", pool);

    assert!(dal.find_by_id(ctx, "not-exists").await.unwrap().is_none());
}

#[sqlx::test]
async fn test_create_with_custom_base_url(pool: SqlitePool) {
    crate::service::dao::model_provider::init();
    let dal = crate::service::dal::model_provider::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let provider = ModelProvider::new(
        "Custom OpenAI Compatible".to_string(),
        ProviderType::Custom,
        "custom-model".to_string(),
        "custom-key".to_string(),
        Some("https://custom.api.com/v1".to_string()),
        Some("自定义兼容接口".to_string()),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &provider).await.unwrap();
    let found = dal.find_by_id(ctx, &provider.po.id).await.unwrap().unwrap();

    assert_eq!(found.po.base_url, Some("https://custom.api.com/v1".to_string()));
    assert_eq!(found.po.provider_type, ProviderType::Custom);
}

#[sqlx::test]
async fn test_all_provider_types(pool: SqlitePool) {
    crate::service::dao::model_provider::init();
    let dal = crate::service::dal::model_provider::dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let cases = vec![
        (ProviderType::OpenAI, "OpenAI"),
        (ProviderType::Custom, "Custom"),
        (ProviderType::DeepSeek, "DeepSeek"),
        (ProviderType::Doubao, "Doubao"),
        (ProviderType::Qwen, "Qwen"),
        (ProviderType::Ollama, "Ollama"),
    ];

    for (ptype, name) in cases {
        let provider = ModelProvider::new(
            name.to_string(),
            ptype,
            "model".to_string(),
            "key".to_string(),
            None,
            None,
            "admin".to_string(),
        );
        dal.create(ctx.clone(), &provider).await.unwrap();
    }

    let all = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 6);
}
