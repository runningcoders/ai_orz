//! OpenAI 原生 Cortex 实现

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig::prelude::*;
use rig::agent::Agent;
use rig::completion::Prompt;
use rig::tool::ToolDyn;
use rig::providers::openai;
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use crate::models::brain::CortexTrait;
use crate::pkg::request_context::RequestContext;

/// OpenAI 原生 Cortex
#[derive(Clone)]
pub struct OpenAiCortex {
    agent: Agent<ResponsesCompletionModel>,
}

impl OpenAiCortex {
    pub fn new(
        api_key: String, 
         model: String,
         base_url: Option<String>,
         rig_tools: Vec<Box<dyn ToolDyn>>,
    ) -> Result<Self> {
        let builder = openai::Client::builder().api_key(api_key);

        let builder = if let Some(base_url) = base_url {
            builder.base_url(base_url)
        } else {
            builder
        };

        let client = builder.build()
            .map_err(|e| anyhow!("Failed to build OpenAI client: {}", e))?;

        // 使用指定模型创建 Agent
        let agent = if rig_tools.is_empty() {
            client.agent(model).build()
        } else {
            client.agent(model).tools(rig_tools).build()
        };

        Ok(Self { agent })
    }
}

#[async_trait]
impl CortexTrait for OpenAiCortex {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response: Result<String, _> = self.agent.prompt(prompt).await;
        response.map_err(|e| anyhow!("OpenAI prompt failed: {}", e))
    }

    fn support_tools(&self) -> bool {
        true
    }
}
