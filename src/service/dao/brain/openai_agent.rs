//! OpenAI 原生 rig Agent 实现

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig::prelude::*;
use rig::providers::openai::{Client};
use super::*;

/// OpenAI 原生 Rig Agent
pub struct OpenAiRigAgent {
    agent: rig::agent::Agent<Client, ()>,
}

impl OpenAiRigAgent {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        let client = if let Some(base_url) = base_url {
            Client::new(api_key).with_base_url(base_url)
        } else {
            Client::new(api_key)
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
