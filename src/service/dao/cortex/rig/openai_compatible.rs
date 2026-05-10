//! OpenAI 兼容模式 Cortex 实现
//! 兼容 OpenAI API 格式的第三方服务

use async_trait::async_trait;
use anyhow::{Result, anyhow};
use common::enums::ModelCapability;
use rig::prelude::*;
use rig::agent::Agent;
use rig::completion::Prompt;
use rig::tool::ToolDyn;
use rig::providers::openai;
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use rig::embeddings::EmbeddingModel;
use crate::models::brain::CortexTrait;
use crate::pkg::request_context::RequestContext;
/// OpenAI 兼容模式 Cortex - Agent 类型
#[derive(Clone)]
pub struct OpenAiCompatibleCortex {
    client: openai::Client,
    embedding_model: String,
    agent: Agent<ResponsesCompletionModel>,
}

impl OpenAiCompatibleCortex {
    pub fn new(
        api_key: String,
        model: String,
        default_base_url: String,
         user_base_url: Option<String>,
         rig_tools: Vec<Box<dyn ToolDyn>>,
    ) -> Result<Self> {
        let base_url = user_base_url.unwrap_or(default_base_url);

        let builder = openai::Client::builder().api_key(api_key).base_url(base_url);

        let client = builder.build()
            .map_err(|e| anyhow!("Failed to build OpenAI compatible client: {}", e))?;

        // 使用指定模型创建 Agent
        let agent = if rig_tools.is_empty() {
            client.agent(model.clone()).build()
        } else {
            client.agent(model.clone()).tools(rig_tools).build()
        };

        Ok(Self {
            client,
            embedding_model: model.clone(), // 使用 ModelProvider 指定的模型
            agent,
        })
    }
}

#[async_trait]
impl CortexTrait for OpenAiCompatibleCortex {
    fn capability(&self) -> ModelCapability {
        ModelCapability::Agent
    }

    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response: Result<String, _> = self.agent.prompt(prompt).await;
        response.map_err(|e| anyhow!("OpenAI compatible prompt failed: {}", e))
    }

    async fn embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // ✅ 真正调用 OpenAI 兼容的 embedding API（rig-core 0.34+）
        let embedding_model = self.client.embedding_model(&self.embedding_model);
        let embeddings = embedding_model
            .embed_texts(texts.to_vec())
            .await
            .map_err(|e| anyhow!("OpenAI compatible embeddings failed: {}", e))?;
        
        // 提取向量数据: Vec<Embedding> -> Vec<Vec<f32>>
        let vectors = embeddings
            .into_iter()
            .map(|e| e.vec.into_iter().map(|x| x as f32).collect())
            .collect();

        Ok(vectors)
    }

    fn support_tools(&self) -> bool {
        true
    }
}
