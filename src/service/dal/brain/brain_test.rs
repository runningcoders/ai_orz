//! BrainDal 单元测试

use crate::models::brain::{Brain, Cortex, CortexTrait, CoreMemory, Memory};
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use crate::pkg::RequestContext;
use crate::service::dal::brain::{BrainDal, BrainDalTrait};
use anyhow::Result;
use async_trait::async_trait;

/// 一个简单的模拟 CortexTrait 实现用于测试
struct MockCortexTrait;

#[async_trait]
impl CortexTrait for MockCortexTrait {
    async fn prompt(&self, _prompt: &str) -> Result<String> {
        Ok("mock response".to_string())
    }

    fn support_tools(&self) -> bool {
        false
    }
}

/// 测试 BrainDal.wake_brain 正常创建 Brain
#[test]
fn test_wake_brain_success() {
    // 1. 创建 ModelProviderPo 和 ModelProvider
    let po = ModelProviderPo {
        id: "test-id".to_string(),
        name: "test".to_string(),
        provider_type: "OpenAI".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        model_name: "gpt-4o".to_string(),
        status: 1,
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };
    let model_provider = ModelProvider::new(po);

    // 2. 创建 Cortex
    let cortex = Cortex::new(model_provider, Box::new(MockCortexTrait));

    // 3. 创建 Memory
    let memory = Memory::new(
        "你是一个有用的AI助手".to_string(),
        "[\"聊天\", \"问答\"]".to_string(),
    );

    // 4. 创建 ctx
    let ctx = RequestContext::new("test-user".to_string());

    // 5. 调用 wake_brain
    let dal = BrainDal::new();
    let result = dal.wake_brain(ctx, cortex, memory);

    // 6. 验证结果
    assert!(result.is_ok());
    let brain = result.unwrap();

    // 验证 Brain 持有 cortex 和 memory
    assert_eq!(brain.cortex().model_provider.po.id, "test-id");
    assert_eq!(brain.memory.core.soul, "你是一个有用的AI助手");
    assert_eq!(brain.memory.core.capabilities, "[\"聊天\", \"问答\"]");
}
