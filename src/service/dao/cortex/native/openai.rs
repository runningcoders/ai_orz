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
        options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<ThinkResult> {
        http::call_chat_completions(ctx, &self.client, provider, messages, tools, options).await
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
        //
        // 耗时（`duration_ms`）在本路径**逐请求精确计时**：
        //
        // 上一版这里写着「一次 HTTP 请求会产出多条事件、耗时无法一对一归属」——
        // 那是误读，已在 `http.rs` 两条路径上核对过 as-built 事实：
        //   · 标准 `/embeddings`：一次请求 → 恰好一条 usage（`input` 是数组不是逐条请求）
        //   · DoubaoVision `/embeddings/multimodal`：逐条文本各发一次请求 → 各一条 usage
        // 即 **事件条数 == 真实 HTTP 请求数**，`EmbeddingOutcome.durations_ms` 与
        // `usages` 同长同序，可以一对一挂上去。于是 `SUM` / `COUNT` / `AVG` 三者同时
        // 正确，不需要「总时间 ÷ 总次数」的近似口径。
        //
        // `durations_ms.get(idx)` 而非直接下标：万一日后两条 push 出现不同步，
        // 退化路径是「这一条没有耗时」（落 NULL）而不是丢掉整条事件——
        // 事件条数等于请求数是更硬的契约，不能因为耗时缺失而少记调用次数。
        let now = chrono::Utc::now().timestamp_millis();
        for (idx, usage) in outcome.usages.iter().enumerate() {
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
                .with_reasoning_tokens(usage.reasoning_tokens)
                .with_duration_ms(outcome.durations_ms.get(idx).copied());
            let _ = crate::record_event!(ctx, event);
        }

        Ok(outcome.vectors)
    }
}
