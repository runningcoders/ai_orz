//! Ollama 本地 Agent 实现
//!
//! 本地运行大模型，不需要云端 API

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig::prelude::*;
use rig::providers::ollama::{Client};
use crate::models::brain::{self, RigAgent};

/// Ollama 本地 Rig Agent
pub struct OllamaRigAgent {
    agent: rig::agent::Agent<Client, ()>,
}

impl OllamaRigAgent {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        let client = if let Some(base_url) = base_url {
            Client::new(base_url)
        } else {
            Client::default()
        };
        
        // Ollama 不需要 api key，但 rig 的接口需要，所以忽略
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
