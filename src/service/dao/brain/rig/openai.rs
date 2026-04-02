//! OpenAI 原生 Cortex 实现

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig_core::prelude::*;
use crate::models::brain::{self, Cortex};

/// OpenAI 原生 Cortex
pub struct OpenAiCortex {
    agent: rig_core::agent::Agent<rig_core::providers::openai::Client, ()>,
}

impl OpenAiCortex {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        let client = if let Some(base_url) = base_url {
            rig_core::providers::openai::Client::new(api_key).with_base_url(base_url)
        } else {
            rig_core::providers::openai::Client::new(api_key)
        };
        
        // 使用指定模型创建 Agent
        let agent = client.agent(model).build();
        
        Ok(Self { agent })
    }
}

#[async_trait]
impl Cortex for OpenAiCortex {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response = self.agent.prompt(prompt).await
            .map_err(|e| anyhow!("OpenAI prompt failed: {}", e))?;
        
        Ok(response)
    }
    
    fn support_tools(&self) -> bool {
        true
    }
}
