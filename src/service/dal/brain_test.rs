//! Brain DAL 单元测试
//!
//! 测试 Brain DAL 的 wake_brain 和 test_connection 功能

use crate::service::dal::brain::{self, BrainDal, BrainDalTrait};
use crate::models::{self, brain::*, model_provider::*};
use common::enums::ProviderType;
use common::constants::RequestContext;
use uuid::Uuid;
use crate::pkg::storage;
use crate::service::dao::cortex;

/// 测试所有 Brain DAL 功能
/// 
/// 由于 storage 使用全局 OnceLock 只能初始化一次，
/// 所以所有测试放在一个函数中顺序执行。
#[tokio::test]
async fn test_all_brain_dal_functions() {
    // 使用随机文件名，避免冲突
    let random_name = format!("/tmp/ai_orz_test_brain_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 初始化 cortex dao
    cortex::init();

    // 初始化 brain dal
    brain::init(cortex::dao());

    let ctx = RequestContext::new(Some("test-user".to_string()), None);
    let brain_dal = brain::dal();

    // ========== 测试 1: wake_brain ==========
    let provider_po = ModelProviderPo::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        "gpt-4o".to_string(),
        "test-key".to_string(),
        Some("https://api.openai.com/v1".to_string()),
        "OpenAI GPT-4o Official".to_string(),
        "test".to_string(),
    );

    let provider = ModelProvider::from_po(provider_po);

    // 创建 Memory
    let memory = Memory::new(
        "你是一个有用的AI助手".to_string(),
        "[\"聊天\", \"问答\"]".to_string(),
    );

    let result = brain_dal.wake_brain(ctx.clone(), &provider, memory);
    
    // 应该能成功创建，API key 不正确只会在实际调用时失败，创建本身不会失败
    assert!(result.is_ok());

    // ========== 测试 2: test_connection ==========
    let provider_po = ModelProviderPo::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        "gpt-4o".to_string(),
        "test-key".to_string(),
        Some("https://api.openai.com/v1".to_string()),
        "OpenAI GPT-4o Official".to_string(),
        "test".to_string(),
    );

    let provider = ModelProvider::from_po(provider_po);

    // 即使 API key 不正确，test_connection 也应该完成路径调用，只是因为 API 调用失败返回 Err，这是预期的
    let result = brain_dal.test_connection(ctx, &provider, "Hello!");
    
    // 创建 Cortex 成功（我们只验证路径），调用会失败因为 API key 不对，这是预期的
    assert!(result.is_err());
}
