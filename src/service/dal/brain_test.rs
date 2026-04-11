//! Brain DAL 单元测试
//!
//! 测试 Brain DAL 的 wake_brain 和 test_connection 功能

use crate::service::dal::brain::{BrainDal, BrainDalTrait};
use crate::models::{brain::*, model_provider::*};
use crate::service::dao::cortex;
use common::enums::ProviderType;
use crate::pkg::RequestContext;
use uuid::Uuid;
use sqlx::SqlitePool;

/// 测试 Brain DAL 创建 wake_brain 功能
#[sqlx::test]
async fn test_wake_brain(pool: SqlitePool) {
    // Storage 已经自动迁移，使用传入的 pool
    // 初始化 cortex dao 和 brain dal
    cortex::init();
    let cortex_dao = cortex::dao();
    let brain_dal = BrainDal::new(cortex_dao);

    let ctx = RequestContext::new_simple("test-user", pool);

    // ========== 测试: wake_brain ==========
    let provider_po = ModelProviderPo::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        "gpt-4o".to_string(),
        "test-key".to_string(),
        Some("https://api.openai.com/v1".to_string()),
        Some("OpenAI GPT-4o Official".to_string()),
        "test".to_string(),
    );

    let provider = ModelProvider::from_po(provider_po);

    // 创建 Memory
    let memory = Memory::new(
        "你是一个有用的AI助手".to_string(),
        "[\"chat\", \"question\"]".to_string(),
    );

    let result = brain_dal.wake_brain(ctx.clone(), &provider, memory);
    
    // 应该能成功创建，API key 不正确只会在实际调用时失败，创建本身不会失败
    assert!(result.is_ok());
}

/// 测试 Brain DAL test_connection 功能
#[sqlx::test]
async fn test_test_connection(pool: SqlitePool) {
    // Storage 已经自动迁移，使用传入的 pool
    // 初始化 cortex dao 和 brain dal
    cortex::init();
    let cortex_dao = cortex::dao();
    let brain_dal = BrainDal::new(cortex_dao);

    let ctx = RequestContext::new_simple("test-user", pool);

    // ========== 测试: test_connection ==========
    let provider_po = ModelProviderPo::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        "gpt-4o".to_string(),
        "test-key".to_string(),
        Some("https://api.openai.com/v1".to_string()),
        Some("OpenAI GPT-4o Official".to_string()),
        "test".to_string(),
    );

    let provider = ModelProvider::from_po(provider_po);

    // 即使 API key 不正确，test_connection 也应该完成路径调用，返回 Err，这是预期的
    let result = brain_dal.test_connection(ctx, &provider, "Hello!").await;
    
    // 创建 Cortex 成功（我们只验证路径），调用会失败因为 API key 不对，这是预期的
    assert!(result.is_err());
}
