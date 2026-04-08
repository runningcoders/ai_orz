//! Brain DAL 单元测试
//!
//! 测试 Brain DAL 的 wake_brain 和 test_connection 功能

use super::*;
use crate::models::{self, brain::*, model_provider::*};
use crate::pkg::constants::provider_type::ProviderType;
use crate::pkg::RequestContext;

#[test]
fn test_wake_brain() {
    let ctx = RequestContext::new("test-user".to_string());
    let provider_po = ModelProviderPo {
        id: "test-id-1".to_string(),
        name: "OpenAI GPT-4o".to_string(),
        provider_type: "OpenAI".to_string(),
        model_name: "gpt-4o".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        status: 1,
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::new(provider_po);

    // 创建 Memory
    let memory = Memory::new(
        "你是一个有用的AI助手".to_string(),
        "[\"聊天\", \"问答\"]".to_string(),
    );

    let brain_dal = dal();
    let result = brain_dal.wake_brain(ctx, &provider, memory);
    
    // 应该能成功创建，API key 不正确只会在实际调用时失败，创建本身不会失败
    assert!(result.is_ok());
}

#[test]
fn test_test_connection_returns_result() {
    let ctx = RequestContext::new("test-user".to_string());
    let provider_po = ModelProviderPo {
        id: "test-id-1".to_string(),
        name: "OpenAI GPT-4o".to_string(),
        provider_type: "OpenAI".to_string(),
        model_name: "gpt-4o".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        status: 1,
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let provider = ModelProvider::new(provider_po);

    let brain_dal = dal();
    // 即使 API key 不正确，test_connection 应该返回 Err，这是预期的
    // 我们只测试 API 调用路径正确，不测试实际 API 调用成功
    let result = brain_dal.test_connection(ctx, &provider, "Hello!");
    
    // 创建 Cortex 成功（我们只验证路径），调用会失败因为 API key 不对，这是预期的
    assert!(result.is_err());
}
