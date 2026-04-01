//! Brain - 大脑工厂
//!
//! 根据 Agent 配置创建对应的 rig Agent，处理与外部 LLM 提供商的通信

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig::prelude::*;
use crate::models::{agent::Agent, model_provider::ModelProviderPo};

/// 大脑封装了 rig Agent，负责与外部 LLM 提供商通信
pub struct Brain {
    agent: Box<dyn RigAgent + Send + Sync>,
}

/// 统一的 Rig Agent  trait
#[async_trait]
pub trait RigAgent {
    /// 运行 prompt，获取回答
    async fn prompt(&self, prompt: &str) -> Result<String>;
    
    /// 是否支持工具调用
    fn support_tools(&self) -> bool;
}

impl Brain {
    /// 根据 ModelProvider 创建大脑
    pub fn create_from_model_provider(model_provider: &ModelProviderPo) -> Result<Self> {
        use crate::models::model_provider::ProviderType;
        
        let api_key = model_provider.api_key.clone();
        let model = model_provider.model_name.clone();
        let base_url = model_provider.base_url.clone();
        
        let agent: Box<dyn RigAgent + Send + Sync> = match model_provider.provider_type {
            ProviderType::OpenAi => Box::new(OpenAiRigAgent::new(api_key, model, base_url)?),
            ProviderType::DeepSeek => Box::new(OpenAiCompatibleAgent::new(api_key, model, "https://api.deepseek.com".to_string(), base_url)?),
            ProviderType::Qwen => Box::new(OpenAiCompatibleAgent::new(api_key, model, "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(), base_url)?),
            ProviderType::Doubao => Box::new(OpenAiCompatibleAgent::new(api_key, model, "https://ark.cn-beijing.volces.com/api".to_string(), base_url)?),
            ProviderType::Ollama => Box::new(OllamaRigAgent::new(api_key, model, base_url)?),
            ProviderType::OpenAiCompatible => Box::new(OpenAiCompatibleAgent::new(api_key, model, "".to_string(), base_url)?),
        };
        
        Ok(Self { agent })
    }
    
    /// 运行 prompt
    pub async fn prompt(&self, prompt: &str) -> Result<String> {
        self.agent.prompt(prompt).await
    }
}

mod openai_agent;
mod openai_compatible;
mod ollama_agent;

pub use openai_agent::OpenAiRigAgent;
pub use openai_compatible::OpenAiCompatibleAgent;
pub use ollama_agent::OllamaRigAgent;
