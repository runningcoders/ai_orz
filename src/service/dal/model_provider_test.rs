//! Model Provider DAL 单元测试

use crate::service::dal::model_provider::{ModelProviderDal, ModelProviderDalTrait};
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use common::enums::ProviderType;
use crate::pkg::storage;
use crate::pkg::storage::sql;
use common::constants::RequestContext;
use crate::service::dao::model_provider;
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

fn setup_test_dal() -> Arc<dyn ModelProviderDalTrait> {
    // 使用随机文件名，避免冲突
    let random_name = format!("/tmp/ai_orz_test_mp_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表
    let _ = storage::get().conn().execute(sql::SQLITE_CREATE_TABLE_MODEL_PROVIDERS, ());

    // 初始化 DAO
    model_provider::init();

    Arc::new(ModelProviderDal::new(model_provider::dao()))
}

/// 测试所有 Model Provider DAL 功能
/// 
/// 由于 storage 使用全局 OnceLock 只能初始化一次，
/// 所以所有测试放在一个函数中顺序执行。
#[test]
fn test_all_model_provider_dal_functions() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    // ========== 测试 1: 创建并查询
    let provider = ModelProvider::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        "gpt-4o".to_string(),
        "sk-xxx".to_string(),
        None,
        "OpenAI GPT-4o 官方模型".to_string(),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &provider).unwrap();
    let found = dal.find_by_id(ctx.clone(), &provider.po.id).unwrap().unwrap();

    assert_eq!(found.po.name, "OpenAI GPT-4o");
    assert_eq!(found.po.provider_type, ProviderType::OpenAI);
    assert_eq!(found.po.model_name, "gpt-4o");
    assert_eq!(found.po.created_by, "admin");

    // ========== 测试 2: 查询全部
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
        dal.create(ctx.clone(), &provider).unwrap();
    }

    let all = dal.find_all(ctx.clone()).unwrap();
    assert_eq!(all.len(), 4); // 1 + 3 = 4

    // ========== 测试 3: 更新
    let mut updated = found.clone();
    updated.po.name = "Updated".to_string();
    updated.po.model_name = "gpt-4o-mini".to_string();
    updated.touch("editor");

    dal.update(new_ctx("editor"), &updated).unwrap();

    let found_after_update = dal.find_by_id(ctx.clone(), &updated.po.id).unwrap().unwrap();
    assert_eq!(found_after_update.po.name, "Updated");
    assert_eq!(found_after_update.po.model_name, "gpt-4o-mini");
    assert_eq!(found_after_update.po.modified_by, "editor");

    // ========== 测试 4: 删除
    assert!(dal.delete(ctx.clone(), &updated).is_ok());
    let found_after_delete = dal.find_by_id(ctx.clone(), &updated.po.id).unwrap();
    assert!(found_after_delete.is_none());

    // ========== 测试 5: 查询不存在
    let found_none = dal.find_by_id(ctx.clone(), "not-exist-id").unwrap();
    assert!(found_none.is_none());

    // ========== 测试 6: 自定义 Base URL
    let provider = ModelProvider::new(
        "Custom OpenAI Compatible".to_string(),
        ProviderType::Custom,
        "custom-model".to_string(),
        "custom-key".to_string(),
        Some("https://custom.api.com/v1".to_string()),
        "自定义兼容接口".to_string(),
        "admin".to_string(),
    );

    dal.create(ctx.clone(), &provider).unwrap();
    let found = dal.find_by_id(ctx.clone(), &provider.po.id).unwrap().unwrap();

    assert_eq!(found.po.base_url, Some("https://custom.api.com/v1".to_string()));
    assert_eq!(found.po.provider_type, ProviderType::Custom);

    // ========== 测试 7: 所有 Provider 类型都能正常创建
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
        dal.create(ctx.clone(), &provider).unwrap();
    }

    let all = dal.find_all(ctx.clone()).unwrap();
    // 之前有 4，加上这里 6 个，总共 10
    assert_eq!(all.len(), 10);
}
