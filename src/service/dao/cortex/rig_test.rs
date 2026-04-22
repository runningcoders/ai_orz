//! Cortex DAO 测试

use super::*;
use crate::models::{model_provider::{ModelProvider, ModelProviderPo}, tool::FullTool};
use common::enums::ProviderType;
use crate::pkg::request_context::RequestContext;
use sqlx::SqlitePool;

#[sqlx::test]
async fn test_create_openai_cortex(pool: SqlitePool) {
    let ctx = RequestContext::new_simple("test-user", pool);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "OpenAI GPT-4o".to_string(),
        provider_type: ProviderType::OpenAI,
        model_name: "gpt-4o".to_string(),
        api_key: "test-key".to_string(),
        base_url: Some("".to_string()),
        description: Some("OpenAI GPT-4o 模型".to_string()),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let tools: Vec<FullTool> = vec![];
    let result = dao.create_cortex_trait(ctx, &provider, tools);
    
    // 应该能成功创建，API key 不正确只会在运行时失败，创建本身不会失败
    assert!(result.is_ok());
}

#[sqlx::test]
async fn test_create_deepseek_cortex(pool: SqlitePool) {
    let ctx = RequestContext::new_simple("test-user", pool);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "DeepSeek".to_string(),
        provider_type: ProviderType::DeepSeek,
        model_name: "deepseek-chat".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        description: Some("DeepSeek 对话模型".to_string()),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let tools: Vec<FullTool> = vec![];
    let result = dao.create_cortex_trait(ctx, &provider, tools);
    
    assert!(result.is_ok());
}

#[sqlx::test]
async fn test_create_qwen_cortex(pool: SqlitePool) {
    let ctx = RequestContext::new_simple("test-user", pool);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "Qwen".to_string(),
        provider_type: ProviderType::Qwen,
        model_name: "qwen-turbo".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        description: Some("通义千问 turbo".to_string()),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let tools: Vec<FullTool> = vec![];
    let result = dao.create_cortex_trait(ctx, &provider, tools);
    
    assert!(result.is_ok());
}

#[sqlx::test]
async fn test_create_doubao_cortex(pool: SqlitePool) {
    let ctx = RequestContext::new_simple("test-user", pool);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "Doubao".to_string(),
        provider_type: ProviderType::Doubao,
        model_name: "doubao-pro".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        description: Some("字节跳动豆包".to_string()),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let tools: Vec<FullTool> = vec![];
    let result = dao.create_cortex_trait(ctx, &provider, tools);
    
    assert!(result.is_ok());
}

#[sqlx::test]
async fn test_create_ollama_cortex(pool: SqlitePool) {
    let ctx = RequestContext::new_simple("test-user", pool);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "Ollama Llama3".to_string(),
        provider_type: ProviderType::Ollama,
        model_name: "llama3".to_string(),
        api_key: "".to_string(),
        base_url: Some("http://localhost:11434/v1".to_string()),
        description: Some("本地 Llama3".to_string()),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let tools: Vec<FullTool> = vec![];
    let result = dao.create_cortex_trait(ctx, &provider, tools);
    
    assert!(result.is_ok());
}

#[sqlx::test]
async fn test_create_openai_compatible_custom_base_url(pool: SqlitePool) {
    let ctx = RequestContext::new_simple("test-user", pool);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "Custom OpenAI Compatible".to_string(),
        provider_type: ProviderType::Custom,
        model_name: "custom-model".to_string(),
        api_key: "test-key".to_string(),
        base_url: Some("https://custom.api.com/v1".to_string()),
        description: Some("自定义兼容接口".to_string()),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let tools: Vec<FullTool> = vec![];
    let result = dao.create_cortex_trait(ctx, &provider, tools);
    
    assert!(result.is_ok());
}
