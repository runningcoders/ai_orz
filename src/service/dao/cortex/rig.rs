//! Rig 驱动的 Cortex 实现

use anyhow::{Context, Result};
use crate::models::{brain::*, model_provider::ModelProvider};
use common::constants::RequestContext;
use common::enums::ProviderType;
use rig::client::*;
use rig::completion::*;
use std::sync::{Arc, OnceLock};

// ==================== 公开 ====================

/// Rig 驱动的 Cortex DAO 实现
pub struct RigCortexDao {
}

static CORTEX_DAO: OnceLock<Arc<RigCortexDao>> = OnceLock::new();

/// 获取 Cortex DAO 单例
pub fn dao() -> Arc<RigCortexDao> {
    CORTEX_DAO.get().unwrap().clone()
}

/// 初始化 Cortex DAO
pub fn init() {
    let _ = CORTEX_DAO.set(Arc::new(RigCortexDao::new()));
}

impl RigCortexDao {
    pub fn new() -> Self {
        Self {
        }
    }
}

#[async_trait::async_trait]
impl crate::service::dao::cortex::CortexDao for RigCortexDao {
    fn create_cortex_trait(&self, _ctx: RequestContext, provider: &ModelProvider) -> Result<Box<dyn CortexTrait + Send + Sync>> {
        let api_key = provider.po.api_key.clone();
        let model = provider.po.model_name.clone();
        let base_url = provider.po.base_url.clone();

        let cortex: Box<dyn CortexTrait + Send + Sync> = match provider.po.provider_type {
            ProviderType::OpenAI => Box::new(
                self::openai::OpenAiCortex::new(api_key, model, base_url)?
            ),
            ProviderType::DeepSeek => Box::new(
                self::openai_compatible::OpenAiCompatibleCortex::new(api_key, model, "https://api.deepseek.com".to_string(), base_url)?
            ),
            ProviderType::Qwen => Box::new(
                self::openai_compatible::OpenAiCompatibleCortex::new(api_key, model, "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(), base_url)?
            ),
            ProviderType::Doubao => Box::new(
                self::openai_compatible::OpenAiCompatibleCortex::new(api_key, model, "https://ark.cn-beijing.volces.com/api".to_string(), base_url)?
            ),
            ProviderType::Ollama => Box::new(
                self::ollama::OllamaCortex::new(api_key, model, base_url)?
            ),
            ProviderType::Custom => Box::new(
                self::openai_compatible::OpenAiCompatibleCortex::new(api_key, model, "".to_string(), base_url)?
            ),
        };

        Ok(cortex)
    }

    async fn prompt(&self, _ctx: RequestContext, cortex: &dyn CortexTrait, prompt: &str) -> Result<String> {
        cortex.prompt(prompt).await
    }
}

// 具体不同提供商的 Cortex 实现
pub mod openai;
pub mod openai_compatible;
pub mod ollama;
