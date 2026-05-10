//! OpenAI 原生 Cortex 实现

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

/// OpenAI 原生 Cortex - Agent 类型，支持对话和向量
#[derive(Clone)]
pub struct OpenAiCortex {
    client: openai::Client,
    embedding_model: String,
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
impl CortexTrait for OpenAiCortex {
    fn capability(&self) -> ModelCapability {
        ModelCapability::Agent
    }

    async fn prompt(&self, prompt: &str) -> Result<String> {
        let response: Result<String, _> = self.agent.prompt(prompt).await;
        response.map_err(|e| anyhow!("OpenAI prompt failed: {}", e))
    }

    async fn embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // ✅ 真正调用 OpenAI embedding API（rig-core 0.34+）
        let embedding_model = self.client.embedding_model(&self.embedding_model);
        let embeddings = embedding_model
            .embed_texts(texts.to_vec())
            .await
            .map_err(|e| anyhow!("OpenAI embeddings failed: {}", e))?;
        
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
