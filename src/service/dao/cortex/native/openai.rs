//! OpenAI 兼容 CortexDao 实现（单例，仅持有共享 reqwest::Client）

use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::pkg::stats::ModelCallEvent;
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
        ctx: RequestContext,
        provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        // DoubaoVision 走 /embeddings/multimodal；其他 provider 走标准 /embeddings
        let outcome = if provider.provider_type == ProviderType::DoubaoVision {
            http::call_embeddings_multimodal(&self.client, provider, texts).await?
        } else {
            http::call_embeddings(&self.client, provider, texts).await?
        };

        // 向量模型被上层多处调用，统一在最底层收口打点：
        // 一次 HTTP 请求一条事件；provider 未上报 usage 时 token 为 0，仅计调用次数。
        let now = chrono::Utc::now().timestamp_millis();
        for usage in &outcome.usages {
            let event = ModelCallEvent::new(now)
                .with_call_kind(Some("embedding".to_string()))
                .with_agent_id(ctx.agent_id.clone())
                .with_project_id(ctx.project_id.clone())
                .with_task_id(ctx.task_id.clone())
                .with_model_provider_id(Some(provider.id.clone()))
                .with_model_name(Some(provider.model_name.clone()))
                .with_organization_id(ctx.organization_id.clone())
                .with_user_id(ctx.user_id.clone())
                .with_tokens_input(usage.input_tokens)
                .with_tokens_output(usage.output_tokens)
                .with_total_tokens(usage.total())
                .with_cached_tokens(usage.cached_tokens)
                .with_reasoning_tokens(usage.reasoning_tokens);
            let _ = crate::record_event!(ctx, event);
        }

        Ok(outcome.vectors)
    }
}
