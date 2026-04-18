//! Ollama 本地 Cortex 实现

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig::prelude::*;
use rig::agent::Agent;
use rig::completion::Prompt;
use rig::tool::ToolDyn;
use rig::providers::openai;
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use crate::models::brain::CortexTrait;
use crate::models::tool::Tool;

/// Ollama 本地 Cortex
#[derive(Clone)]
pub struct OllamaCortex {
    agent: Agent<ResponsesCompletionModel>,
}

impl OllamaCortex {
    pub fn new(api_key: String, model: String, base_url: Option<String>, tools: Vec<Tool>) -> Result<Self> {
        // Ollama 默认地址 http://localhost:11434/v1
        let default_base_url = "http://localhost:11434/v1".to_string();
        let base_url = base_url.unwrap_or(default_base_url);

        let builder = openai::Client::builder().api_key(api_key).base_url(base_url);

        let client = builder.build()
            .map_err(|e| anyhow!("Failed to build Ollama client: {}", e))?;

        // Extract pre-built tools from Tool struct (already built by ToolDao from registry)
        // Rig expects Box<dyn ToolDyn>, our tools are already Send + Sync which is fine
        let tool_boxes: Vec<Box<dyn ToolDyn>> = tools
            .into_iter()
            .map(|t| unsafe {
                // SAFETY: We know all tools implement Send + Sync, transmute is safe here
                std::mem::transmute(t.tool)
            })
            .collect();

        // 使用指定模型创建 Agent
        let agent = if tool_boxes.is_empty() {
            client.agent(model).build()
        } else {
            client.agent(model).tools(tool_boxes).build()
        };

        Ok(Self { agent })
    }
}

#[async_trait]
impl CortexTrait for OllamaCortex {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response: Result<String, _> = self.agent.prompt(prompt).await;
        response.map_err(|e| anyhow!("Ollama prompt failed: {}", e))
    }

    fn support_tools(&self) -> bool {
        true
    }
}
