//! Cortex DAO 测试

use super::*;
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use common::enums::ProviderType;
use crate::pkg::RequestContext;

#[tokio::test]
async fn test_create_openai_cortex() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "OpenAI GPT-4o".to_string(),
        provider_type: ProviderType::OpenAI,
        model_name: "gpt-4o".to_string(),
        api_key: "test-key".to_string(),
        base_url: "".to_string(),
        description: "OpenAI GPT-4o 模型".to_string(),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let result = dao.create_cortex_trait(ctx, &provider);
    
    // 应该能成功创建，API key 不正确只会在运行时失败，创建本身不会失败
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_deepseek_cortex() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "DeepSeek".to_string(),
        provider_type: ProviderType::DeepSeek,
        model_name: "deepseek-chat".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        description: "DeepSeek 对话模型".to_string(),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let result = dao.create_cortex_trait(ctx, &provider);
    
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_qwen_cortex() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "Qwen".to_string(),
        provider_type: ProviderType::Qwen,
        model_name: "qwen-turbo".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        description: "通义千问 turbo".to_string(),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let result = dao.create_cortex_trait(ctx, &provider);
    
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_doubao_cortex() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "Doubao".to_string(),
        provider_type: ProviderType::Doubao,
        model_name: "doubao-pro".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        description: "字节跳动豆包".to_string(),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let result = dao.create_cortex_trait(ctx, &provider);
    
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_ollama_cortex() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "Ollama Llama3".to_string(),
        provider_type: ProviderType::Ollama,
        model_name: "llama3".to_string(),
        api_key: "".to_string(),
        base_url: Some("http://localhost:11434/v1".to_string()),
        description: "本地 Llama3".to_string(),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let result = dao.create_cortex_trait(ctx, &provider);
    
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_openai_compatible_custom_base_url() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);

    let provider_po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "Custom OpenAI Compatible".to_string(),
        provider_type: ProviderType::Custom,
        model_name: "custom-model".to_string(),
        api_key: "test-key".to_string(),
        base_url: Some("https://custom.api.com/v1".to_string()),
        description: "自定义兼容接口".to_string(),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let dao = rig::RigCortexDao::new();
    let result = dao.create_cortex_trait(ctx, &provider);
    
    assert!(result.is_ok());
}
