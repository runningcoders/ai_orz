//! OpenAI 兼容模式 Cortex 实现
//!
//! 兼容 OpenAI API 格式的第三方服务

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use rig::prelude::*;
use rig::agent::Agent;
use rig::completion::Prompt;
use rig::providers::openai;
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use crate::models::brain::CortexTrait;

/// OpenAI 兼容模式 Cortex
#[derive(Clone)]
pub struct OpenAiCompatibleCortex {
    agent: Agent<ResponsesCompletionModel>,
}

impl OpenAiCompatibleCortex {
    pub fn new(
        api_key: String,
        model: String,
        default_base_url: String,
        user_base_url: Option<String>,
    ) -> Result<Self> {
        let base_url = user_base_url.unwrap_or(default_base_url);

        let builder = openai::Client::builder().api_key(api_key).base_url(base_url);

        let client = builder.build()
            .map_err(|e| anyhow!("Failed to build OpenAI compatible client: {}", e))?;

        // 使用指定模型创建 Agent
        let agent = client.agent(model).build();

        Ok(Self { agent })
    }
}

#[async_trait]
impl CortexTrait for OpenAiCompatibleCortex {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response: Result<String, _> = self.agent.prompt(prompt).await;
        response.map_err(|e| anyhow!("OpenAI compatible prompt failed: {}", e))
    }

    fn support_tools(&self) -> bool {
        true
    }
}
