//! Model Provider DAL 单元测试

use super::*;
use crate::models::model_provider::{ModelProvider, ModelProviderPo, ProviderType};
use crate::pkg::storage;
use crate::service::dao::model_provider;

fn new_ctx(user_id: &str) -> crate::pkg::RequestContext {
    crate::pkg::RequestContext::new(Some(user_id.to_string()), None)
}

fn setup_test_dal() -> Arc<dyn ModelProviderDalTrait> {
    // 初始化内存数据库并创建表
    let conn = storage::test_conn();
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_MODEL_PROVIDERS)
        .unwrap();

    // 初始化 DAO
    model_provider::init();

    Arc::new(ModelProviderDal::new(model_provider::dao()))
}

#[test]
fn test_create_and_find_by_id() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let provider = ModelProvider::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAi,
        "gpt-4o".to_string(),
        "sk-xxx".to_string(),
        None,
        "OpenAI GPT-4o 官方模型".to_string(),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &provider).unwrap();
    let found = dal.find_by_id(ctx, &provider.po.id).unwrap().unwrap();

    assert_eq!(found.po.name, "OpenAI GPT-4o");
    assert_eq!(found.po.provider_type, ProviderType::OpenAi);
    assert_eq!(found.po.model_name, "gpt-4o");
    assert_eq!(found.po.created_by, "admin");
}

#[test]
fn test_find_all() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let providers = vec![
        ("OpenAI", ProviderType::OpenAi, "gpt-4o"),
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
        dal.create(ctx.clone(), &provider).unwrap();
    }

    let all = dal.find_all(ctx).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn test_update() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let provider = ModelProvider::new(
        "Original".to_string(),
        ProviderType::OpenAi,
        "gpt-4".to_string(),
        "key".to_string(),
        None,
        "desc".to_string(),
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &provider).unwrap();

    let mut updated = provider.clone();
    updated.po.name = "Updated".to_string();
    updated.po.model_name = "gpt-4o".to_string();
    updated.touch("editor");

    dal.update(new_ctx("editor"), &updated).unwrap();

    let found = dal.find_by_id(ctx, &updated.po.id).unwrap().unwrap();
    assert_eq!(found.po.name, "Updated");
    assert_eq!(found.po.model_name, "gpt-4o");
    assert_eq!(found.po.modified_by, "editor");
}

#[test]
fn test_delete() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let provider = ModelProvider::new(
        "ToDelete".to_string(),
        ProviderType::OpenAi,
        "gpt-4o".to_string(),
        "key".to_string(),
        None,
        "".to_string(),
        "admin".to_string(),
    );
    dal.create(ctx.clone(), &provider).unwrap();

    dal.delete(ctx.clone(), &provider.po.id).unwrap();
    assert!(dal.find_by_id(ctx, &provider.po.id).unwrap().is_none());
}

#[test]
fn test_find_not_exists() {
    let dal = setup_test_dal();
    let ctx = new_ctx("user1");

    assert!(dal.find_by_id(ctx, "not-exists").unwrap().is_none());
}

#[test]
fn test_create_with_custom_base_url() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let provider = ModelProvider::new(
        "Custom OpenAI Compatible".to_string(),
        ProviderType::OpenAiCompatible,
        "custom-model".to_string(),
        "custom-key".to_string(),
        Some("https://custom.api.com/v1".to_string()),
        "自定义兼容接口".to_string(),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &provider).unwrap();
    let found = dal.find_by_id(ctx, &provider.po.id).unwrap().unwrap();

    assert_eq!(found.po.base_url, Some("https://custom.api.com/v1".to_string()));
    assert_eq!(found.po.provider_type, ProviderType::OpenAiCompatible);
}

#[test]
fn test_all_provider_types() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let cases = vec![
        (ProviderType::OpenAi, "OpenAi"),
        (ProviderType::DeepSeek, "DeepSeek"),
        (ProviderType::Qwen, "Qwen"),
        (ProviderType::Doubao, "Doubao"),
        (ProviderType::Ollama, "Ollama"),
        (ProviderType::OpenAiCompatible, "OpenAiCompatible"),
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
        dal.create(ctx.clone(), &provider).unwrap();
    }

    let all = dal.find_all(ctx).unwrap();
    assert_eq!(all.len(), 6);
}
