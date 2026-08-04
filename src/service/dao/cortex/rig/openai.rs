//! OpenAI 原生 Cortex 实现

use super::*;
use crate::pkg::monitoring::rig_hook::RuntimeMonitoringHook;
use anyhow::anyhow;
use async_trait::async_trait;
use common::enums::ModelCapability;
use common::error::Result;
use rig::agent::Agent;
use rig::completion::Prompt;
use rig::embeddings::EmbeddingModel;
use rig::prelude::*;
use rig::providers::openai;
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use rig::tool::DynamicTool;

/// OpenAI 原生 Cortex - Agent 类型，支持对话和向量
#[derive(Clone)]
pub struct OpenAiCortex {
    client: openai::Client,
    model_provider_id: String,
    model_name: String,
    embedding_model: String,
    agent: Agent<ResponsesCompletionModel>,
}

impl OpenAiCortex {
    pub fn new(
        model_provider_id: String,
        api_key: String,
        model: String,
        base_url: Option<String>,
        ctx: RequestContext,
        rig_tools: Vec<DynamicTool>,
    ) -> Result<Self> {
        let builder = openai::Client::builder().api_key(api_key);

        let builder = if let Some(base_url) = base_url {
            builder.base_url(base_url)
        } else {
            builder
        };

        let client = builder
            .build()
            .map_err(|e| anyhow!("Failed to build OpenAI client: {}", e))?;

        // ✅ 提前初始化好 Agent
        let hook = RuntimeMonitoringHook::new(ctx.clone());
        let agent = if rig_tools.is_empty() {
            client.agent(model.clone()).add_hook(hook).build()
        } else {
            client
                .agent(model.clone())
                .add_hook(hook)
                .dynamic_tools(rig_tools)
                .build()
        };

        Ok(Self {
            client,
            model_provider_id,
            model_name: model.clone(),
            embedding_model: model,
            agent,
        })
    }
}

#[async_trait]
impl CortexTrait for OpenAiCortex {
    fn capability(&self) -> ModelCapability {
        ModelCapability::Agent
    }

    fn model_provider_id(&self) -> &str {
        &self.model_provider_id
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn prompt(&self, prompt: &str) -> anyhow::Result<String> {
        let response = self.agent.prompt(prompt).await;
        response.map_err(|e| anyhow!("OpenAI prompt failed: {}", e))
    }

    async fn embeddings(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
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
