//! Ollama 本地 Cortex 实现
//!
//! 本地运行大模型，不需要云端 API

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig::prelude::*;
use rig::agent::Agent;
use rig::completion::Prompt;
use rig::client::Nothing;
use rig::providers::ollama;
use crate::models::brain::{self, Cortex};

/// Ollama 本地 Cortex
pub struct OllamaCortex {
    agent: Agent<ollama::CompletionModel>,
}

impl OllamaCortex {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        let mut builder = ollama::Client::builder();
        
        if let Some(base_url) = base_url {
            builder = builder.base_url(base_url);
        }
        
        let client = builder.api_key(Nothing).build()
            .map_err(|e| anyhow!("Failed to build Ollama client: {}", e))?;
        
        // Ollama 不需要 api key，但接口需要，所以忽略
        let _ = api_key;
        
        let agent = client.agent(model).build();
        
        Ok(Self { agent })
    }
}

#[async_trait]
impl Cortex for OllamaCortex {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response: Result<String, _> = self.agent.prompt(prompt).await;
        response.map_err(|e| anyhow!("Ollama prompt failed: {}", e))
    }
    
    fn support_tools(&self) -> bool {
        true
    }
}
