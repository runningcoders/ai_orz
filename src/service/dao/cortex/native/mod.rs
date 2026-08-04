//! Native Cortex DAO - 自建的模型调用实现（替代 rig）
//!
//! 提供新的 `CortexDao` trait（与 `crate::service::dao::cortex::CortexDao` 旧 trait 区分），
//! 按 provider_type 路由到具体实现。所有实现都是无状态单例（仅持有共享 reqwest::Client），
//! 所有配置从 `&ModelProviderPo` 读取。

use crate::models::cortex_types::{ChatMessage, ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::models::vector::{VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::enums::ProviderType;
use common::error::Result;
use std::sync::{Arc, OnceLock};

pub mod http;
pub mod openai;

/// Native Cortex DAO trait - 模型调用接口
///
/// 职责：
/// - `think()`: 调用模型推理，返回 ThinkResult（Final 或 ToolCall）
/// - `embed()`: 文本转向量
///
/// 实现是无状态单例（仅持有共享 reqwest::Client），所有配置从 &ModelProviderPo 读取。
#[async_trait]
pub trait CortexDao: Send + Sync {
    /// 调用模型推理
    ///
    /// 返回 ThinkResult::Final（最终回答）或 ThinkResult::ToolCall（工具调用请求）。
    /// 接收完整的 messages 数组（多轮对话历史），确保模型能看到之前的 tool_calls 和 tool 结果。
    async fn think(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        messages: &[ChatMessage],
        tools: &[ToolDescriptor],
    ) -> Result<ThinkResult>;

    /// 文本转向量（原始向量）
    async fn embed(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>>;

    /// 向量化实体（返回完整 VectorIndexParams，用于索引场景）
    async fn embed_entity(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        entity: &dyn Vectorizable,
    ) -> Result<VectorIndexParams> {
        let text = entity.vectorize_text();
        let vectors = self
            .embed(ctx, provider, std::slice::from_ref(&text))
            .await?;
        let vector = vectors.into_iter().next().unwrap_or_default();
        Ok(VectorIndexParams {
            vector,
            content_hash: entity.vector_content_hash(),
            model_provider_id: provider.id.clone(),
            embedding_model: provider.model_name.clone(),
            expire_at: entity.vector_expire_at(),
        })
    }

    /// 向量化搜索关键词（返回完整 VectorIndexParams，用于搜索场景）
    async fn embed_text_for_search(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        text: &str,
    ) -> Result<VectorIndexParams> {
        let vectors = self
            .embed(ctx, provider, std::slice::from_ref(&text.to_string()))
            .await?;
        let vector = vectors.into_iter().next().unwrap_or_default();
        Ok(VectorIndexParams {
            vector,
            content_hash: sha256::digest(text),
            model_provider_id: provider.id.clone(),
            embedding_model: provider.model_name.clone(),
            expire_at: None,
        })
    }
}

// ==================== Registry ====================

/// Cortex DAO 注册表
///
/// 按 provider_type 路由到具体的 CortexDao 实现。
/// 所有实现都是无状态单例。
pub struct CortexDaoRegistry {
    openai_compatible: Arc<openai::OpenAiCompatibleCortexDao>,
    // fastembed 和 external 在后续 Task 6 中添加
}

impl CortexDaoRegistry {
    fn new() -> Self {
        Self {
            openai_compatible: Arc::new(openai::OpenAiCompatibleCortexDao::new()),
        }
    }

    /// 根据 provider_type 获取对应的 CortexDao
    pub fn get(&self, provider_type: ProviderType) -> Arc<dyn CortexDao> {
        match provider_type {
            ProviderType::OpenAI
            | ProviderType::DeepSeek
            | ProviderType::Qwen
            | ProviderType::Doubao
            | ProviderType::DoubaoVision
            | ProviderType::Ollama
            | ProviderType::Custom => self.openai_compatible.clone(),
            // FastEmbed 和 External 在 Task 6 中添加
            ProviderType::FastEmbed => {
                // TODO Task 6: 返回 FastEmbedCortexDao
                self.openai_compatible.clone() // 临时 fallback
            }
        }
    }
}

static REGISTRY: OnceLock<CortexDaoRegistry> = OnceLock::new();

/// 获取 Cortex DAO Registry 单例
pub fn registry() -> &'static CortexDaoRegistry {
    REGISTRY
        .get()
        .expect("CortexDaoRegistry not initialized, call native::init() first")
}

/// 初始化 Cortex DAO Registry
pub fn init() {
    let _ = REGISTRY.set(CortexDaoRegistry::new());
}
