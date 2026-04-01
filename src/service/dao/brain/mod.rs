//! Brain DAO - 大脑工厂
//!
//! 根据 Model Provider 创建 Brain 实体，提供统一推理接口

use anyhow::{Result, anyhow};
use crate::models::{self, brain::*, model_provider::ModelProviderPo};

/// Brain DAO 工厂 trait
#[async_trait::async_trait]
pub trait BrainDao: Send + Sync {
    /// 根据 Model Provider 创建 Brain
    fn create_brain(&self, provider: &ModelProviderPo) -> Result<Brain>;
    
    /// 统一调用：运行 prompt，获取回答
    async fn prompt(&self, brain: &Brain, prompt: &str) -> Result<String>;
}

/// 默认 Brain DAO 工厂实现
pub struct RigBrainDao;

impl RigBrainDao {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl BrainDao for RigBrainDao {
    fn create_brain(&self, provider: &ModelProviderPo) -> Result<Brain> {
        use crate::models::model_provider::ProviderType;
        
        let api_key = provider.api_key.clone();
        let model = provider.model_name.clone();
        let base_url = provider.base_url.clone();
        
        let agent: Box<dyn RigAgent + Send + Sync> = match provider.provider_type {
            ProviderType::OpenAi => Box::new(
                super::openai_agent::OpenAiRigAgent::new(api_key, model, base_url)?
            ),
            ProviderType::DeepSeek => Box::new(
                super::openai_compatible::OpenAiCompatibleAgent::new(
                    api_key, model, "https://api.deepseek.com".to_string(), base_url
                )?
            ),
            ProviderType::Qwen => Box::new(
                super::openai_compatible::OpenAiCompatibleAgent::new(
                    api_key, model, "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(), base_url
                )?
            ),
            ProviderType::Doubao => Box::new(
                super::openai_compatible::OpenAiCompatibleAgent::new(
                    api_key, model, "https://ark.cn-beijing.volces.com/api".to_string(), base_url
                )?
            ),
            ProviderType::Ollama => Box::new(
                super::ollama_agent::OllamaRigAgent::new(api_key, model, base_url)?
            ),
            ProviderType::OpenAiCompatible => Box::new(
                super::openai_compatible::OpenAiCompatibleAgent::new(
                    api_key, model, "".to_string(), base_url
                )?
            ),
        };
        
        Ok(Brain::new(agent))
    }

    async fn prompt(&self, brain: &Brain, prompt: &str) -> Result<String> {
        brain.agent.prompt(prompt).await
    }
}

mod openai_agent;
mod openai_compatible;
mod ollama_agent;

pub use openai_agent::OpenAiRigAgent;
pub use openai_compatible::OpenAiCompatibleAgent;
pub use ollama_agent::OllamaRigAgent;

#[cfg(test)]
mod tests;
