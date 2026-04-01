//! Ollama 本地 Cortex 实现
//!
//! 本地运行大模型，不需要云端 API

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig::prelude::*;
use crate::models::brain::{self, Cortex};

/// Ollama 本地 Cortex
pub struct OllamaCortex {
    agent: rig::agent::Agent<rig::providers::ollama::Client, ()>,
}

impl OllamaCortex {
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
impl Cortex for OllamaCortex {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response = self.agent.prompt(prompt).await
            .map_err(|e| anyhow!("Ollama prompt failed: {}", e))?;
        
        Ok(response)
    }
    
    fn support_tools(&self) -> bool {
        true
    }
}
