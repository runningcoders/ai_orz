//! Rig 具体实现 - 默认 BrainDao 实现

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use crate::models::{self, brain::*};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::constants::ProviderType;

/// 默认 Brain DAO 工厂实现
pub struct RigBrainDao;

impl RigBrainDao {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl super::BrainDao for RigBrainDao {
    fn create_brain(&self, provider: &ModelProviderPo) -> Result<Brain> {
        
        let api_key = provider.api_key.clone();
        let model = provider.model_name.clone();
        let base_url = provider.base_url.clone();
        
        let cortex: Box<dyn Cortex + Send + Sync> = match provider.provider_type {
            ProviderType::OpenAi => Box::new(
                self::openai::OpenAiCortex::new(api_key, model, base_url)?
            ),
            ProviderType::DeepSeek => Box::new(
                self::openai_compatible::OpenAiCompatibleCortex::new(
                    api_key, model, "https://api.deepseek.com".to_string(), base_url
                )?
            ),
            ProviderType::Qwen => Box::new(
                self::openai_compatible::OpenAiCompatibleCortex::new(
                    api_key, model, "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(), base_url
                )?
            ),
            ProviderType::Doubao => Box::new(
                self::openai_compatible::OpenAiCompatibleCortex::new(
                    api_key, model, "https://ark.cn-beijing.volces.com/api".to_string(), base_url
                )?
            ),
            ProviderType::Ollama => Box::new(
                self::ollama::OllamaCortex::new(api_key, model, base_url)?
            ),
            ProviderType::OpenAiCompatible => Box::new(
                self::openai_compatible::OpenAiCompatibleCortex::new(
                    api_key, model, "".to_string(), base_url
                )?
            ),
        };
        
        Ok(Brain::new(cortex))
    }

    async fn prompt(&self, brain: &Brain, prompt: &str) -> Result<String> {
        brain.cortex.prompt(prompt).await
    }
}

// 具体不同提供商的 Cortex 实现
pub mod openai;
pub mod openai_compatible;
pub mod ollama;
