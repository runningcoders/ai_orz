//! OpenAI 兼容 CortexDao 实现（单例，仅持有共享 reqwest::Client）

use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::service::dao::cortex::native::http;
use async_trait::async_trait;
use common::enums::ProviderType;
use common::error::Result;

/// OpenAI 兼容 CortexDao
///
/// 无状态单例：所有配置从 `&ModelProviderPo` 读取，自身仅持有共享的 reqwest::Client。
pub struct OpenAiCompatibleCortexDao {
    client: reqwest::Client,
}

impl OpenAiCompatibleCortexDao {
    pub fn new() -> Self {
        let client = crate::pkg::http::presets::llm()
            .build()
            .expect("Failed to build reqwest client for OpenAiCompatibleCortexDao");
        Self { client }
    }
}

impl Default for OpenAiCompatibleCortexDao {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::CortexDao for OpenAiCompatibleCortexDao {
    async fn think(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        messages: &[crate::models::cortex_types::ChatMessage],
        tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        http::call_chat_completions(ctx, &self.client, provider, messages, tools).await
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        // DoubaoVision 走 /embeddings/multimodal
        if provider.provider_type == ProviderType::DoubaoVision {
            return http::call_embeddings_multimodal(&self.client, provider, texts).await;
        }
        // 其他 provider 走标准 /embeddings
        http::call_embeddings(&self.client, provider, texts).await
    }
}
