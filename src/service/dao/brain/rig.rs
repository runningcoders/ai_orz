//! 具体 Rig Agent 实现

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig::prelude::*;
use crate::models::brain::{self, RigAgent};

/// OpenAI 原生 Rig Agent
pub struct OpenAiRigAgent {
    agent: rig::agent::Agent<rig::providers::openai::Client, ()>,
}

impl OpenAiRigAgent {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        let client = if let Some(base_url) = base_url {
            rig::providers::openai::Client::new(api_key).with_base_url(base_url)
        } else {
            rig::providers::openai::Client::new(api_key)
        };
        
        // 使用指定模型创建 Agent
        let agent = client.agent(model).build();
        
        Ok(Self { agent })
    }
}

#[async_trait]
impl RigAgent for OpenAiRigAgent {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response = self.agent.prompt(prompt).await
            .map_err(|e| anyhow!("OpenAI prompt failed: {}", e))?;
        
        Ok(response)
    }
    
    fn support_tools(&self) -> bool {
        true
    }
}

/// OpenAI 兼容格式的 Rig Agent
/// 
/// 适配 DeepSeek、通义千问、豆包 等兼容 OpenAI 接口的提供商
pub struct OpenAiCompatibleAgent {
    agent: rig::agent::Agent<rig::providers::openai::Client, ()>,
}

impl OpenAiCompatibleAgent {
    pub fn new(api_key: String, model: String, default_base_url: String, custom_base_url: Option<String>) -> Result<Self> {
        let base_url = custom_base_url.unwrap_or(default_base_url);
        let client = rig::providers::openai::Client::new(api_key).with_base_url(base_url);
        
        let agent = client.agent(model).build();
        
        Ok(Self { agent })
    }
}

#[async_trait]
impl RigAgent for OpenAiCompatibleAgent {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response = self.agent.prompt(prompt).await
            .map_err(|e| anyhow!("OpenAI compatible prompt failed: {}", e))?;
        
        Ok(response)
    }
    
    fn support_tools(&self) -> bool {
        true
    }
}

/// Ollama 本地 Rig Agent
pub struct OllamaRigAgent {
    agent: rig::agent::Agent<rig::providers::ollama::Client, ()>,
}

impl OllamaRigAgent {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        let client = if let Some(base_url) = base_url {
            rig::providers::ollama::Client::new(base_url)
        } else {
            rig::providers::ollama::Client::default()
        };
        
        // Ollama 不需要 api key，但接口需要，所以忽略
        let _ = api_key;
        
        let agent = client.agent(model).build();
        
        Ok(Self { agent })
    }
}

#[async_trait]
impl RigAgent for OllamaRigAgent {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response = self.agent.prompt(prompt).await
            .map_err(|e| anyhow!("Ollama prompt failed: {}", e))?;
        
        Ok(response)
    }
    
    fn support_tools(&self) -> bool {
        true
    }
}
