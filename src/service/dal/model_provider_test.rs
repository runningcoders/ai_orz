//! Model Provider DAL 单元测试

use crate::service::dal::model_provider::{ModelProviderDal, ModelProviderDalTrait};
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use common::enums::ProviderType;
use crate::pkg::storage::Storage;
use crate::service::dao::model_provider::{ ModelProviderDaoTrait};
use crate::pkg::RequestContext;
use std::sync::Arc;
use uuid::Uuid;

async fn setup_test_dal() -> Arc<dyn ModelProviderDalTrait> {
    // 使用随机文件名，避免冲突 → 每个测试独立数据库，彻底隔离
    let random_name = format!("/tmp/ai_orz_test_mp_dal_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);

    // Storage 自动运行迁移，创建所有表
    let storage = Storage::new(&random_name).await.expect("Failed to create storage");
    let pool = storage.pool();

    // 初始化 DAO 和 DAL
    let dao = ModelProviderDao::new(pool);
    Arc::new(ModelProviderDal::new(dao))
}

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

#[tokio::test]
async fn test_create_and_find_by_id() {
    let dal = setup_test_dal().await;
    let ctx = new_ctx("admin");

    let provider = ModelProvider::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        "gpt-4o".to_string(),
        "sk-xxx".to_string(),
        None,
        "OpenAI GPT-4o 官方模型".to_string(),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &provider).await.unwrap();
    let found = dal.find_by_id(ctx, &provider.po.id.expect("po has id")).await.unwrap().unwrap();

    assert_eq!(found.po.name, Some("OpenAI GPT-4o".to_string()));
    assert_eq!(found.po.provider_type, ProviderType::OpenAI);
    assert_eq!(found.po.model_name, Some("gpt-4o".to_string()));
    assert_eq!(found.po.created_by, Some("admin".to_string()));
}

#[tokio::test]
async fn test_find_all() {
    let dal = setup_test_dal().await;
    let ctx = new_ctx("admin");

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
            "".to_string(),
            "admin".to_string(),
        );
        dal.create(ctx.clone(), &provider).await.unwrap();
    }

    let all = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn test_update() {
    let dal = setup_test_dal().await;
    let ctx = new_ctx("admin");

    let provider = ModelProvider::new(
        "Original".to_string(),
        ProviderType::OpenAI,
        "gpt-4".to_string(),
        "key".to_string(),
        None,
        "desc".to_string(),
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &provider).await.unwrap();

    let mut updated = provider.clone();
    updated.po.name = Some("Updated".to_string());
    updated.po.model_name = Some("gpt-4o".to_string());
    updated.touch("editor");

    dal.update(new_ctx("editor"), &updated).await.unwrap();

    let found = dal.find_by_id(ctx, &updated.po.id.expect("po has id")).await.unwrap().unwrap();
    assert_eq!(found.po.name, Some("Updated".to_string()));
    assert_eq!(found.po.model_name, Some("gpt-4o".to_string()));
    assert_eq!(found.po.modified_by, Some("editor".to_string()));
}

#[tokio::test]
async fn test_delete() {
    let dal = setup_test_dal().await;
    let ctx = new_ctx("admin");

    let provider = ModelProvider::new(
        "ToDelete".to_string(),
        ProviderType::OpenAI,
        "gpt-4o".to_string(),
        "key".to_string(),
        None,
        "".to_string(),
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &provider).await.unwrap();

    dal.delete(ctx.clone(), &provider).await.unwrap();
    assert!(dal.find_by_id(ctx, &provider.po.id.expect("po has id")).await.unwrap().is_none());
}

#[tokio::test]
async fn test_find_not_exists() {
    let dal = setup_test_dal().await;
    let ctx = new_ctx("user1");

    assert!(dal.find_by_id(ctx, "not-exists").await.unwrap().is_none());
}

#[tokio::test]
async fn test_create_with_custom_base_url() {
    let dal = setup_test_dal().await;
    let ctx = new_ctx("admin");

    let provider = ModelProvider::new(
        "Custom OpenAI Compatible".to_string(),
        ProviderType::Custom,
        "custom-model".to_string(),
        "custom-key".to_string(),
        Some("https://custom.api.com/v1".to_string()),
        "自定义兼容接口".to_string(),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &provider).await.unwrap();
    let found = dal.find_by_id(ctx, &provider.po.id.expect("po has id")).await.unwrap().unwrap();

    assert_eq!(found.po.base_url, Some(Some("https://custom.api.com/v1".to_string())));
    assert_eq!(found.po.provider_type, ProviderType::Custom);
}

#[tokio::test]
async fn test_all_provider_types() {
    let dal = setup_test_dal().await;
    let ctx = new_ctx("admin");

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
            "".to_string(),
            "admin".to_string(),
        );
        dal.create(ctx.clone(), &provider).await.unwrap();
    }

    let all = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 6);
}
