//! Cortex DAL 单元测试
//!
//! 测试 Cortex DAL 的 create_cortex 和 wake_cortex 功能

use super::*;
use crate::models::{self, brain::*, model_provider::*};
use crate::pkg::constants::provider_type::ProviderType;
use crate::pkg::RequestContext;

#[tokio::test]
async fn test_create_cortex() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);
    let provider_po = ModelProviderPo {
        id: "test-id-1".to_string(),
        name: "OpenAI GPT-4o".to_string(),
        provider_type: ProviderType::OpenAI,
        model_name: "gpt-4o".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        description: "OpenAI GPT-4o 模型".to_string(),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let cortex_dal = dal();
    let result = cortex_dal.create_cortex(ctx, &provider);
    
    // 应该能成功创建，API key 不正确只会在运行时失败，创建本身不会失败
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_wake_cortex_returns_result() {
    let ctx = RequestContext::new(Some("test-user".to_string()), None);
    let provider_po = ModelProviderPo {
        id: "test-id-1".to_string(),
        name: "OpenAI GPT-4o".to_string(),
        provider_type: ProviderType::OpenAI,
        model_name: "gpt-4o".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        description: "OpenAI GPT-4o 模型".to_string(),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::from_po(provider_po);

    let cortex_dal = dal();
    // 即使 API key 不正确，create_cortex 也成功，wake_cortex 应该返回 Err，这是预期的
    let result = cortex_dal.wake_cortex(ctx, &provider, "Hello!");
    
    // 创建成功（create_cortex 成功），但调用会失败因为 API key 不对，这是预期的
    // 我们只测试 API 调用路径正确，不测试实际 API 调用成功
    assert!(result.is_err());
    // 错误信息应该包含 API 调用失败
    assert!(result.unwrap_err().to_string().contains("api"));
}
