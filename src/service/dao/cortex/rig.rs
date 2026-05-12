//! Rig 驱动的 Cortex 实现

use anyhow::Result;
use crate::models::{brain::*, model_provider::ModelProviderPo, vector::VectorIndexParams};
use crate::pkg::request_context::RequestContext;
use rig::tool::ToolDyn;
use common::enums::{ModelCapability, ProviderType};
use std::sync::{Arc, OnceLock};

/// Rig 驱动的 Cortex 实现
pub struct RigCortexDao {
}

static CORTEX_DAO: OnceLock<Arc<RigCortexDao>> = OnceLock::new();

/// 获取 Cortex DAO 单例
pub fn dao() -> Arc<RigCortexDao> {
    CORTEX_DAO.get().unwrap().clone()
}

/// 初始化 Cortex DAO
pub fn init() {
    let _ = CORTEX_DAO.set(Arc::new(RigCortexDao::new()));
}

impl RigCortexDao {
    pub fn new() -> Self {
        Self {
        }
    }
}

#[async_trait::async_trait]
impl super::CortexDao for RigCortexDao {
    fn create_cortex_trait(&self, _ctx: RequestContext, provider: &ModelProviderPo, rig_tools: Vec<Box<dyn ToolDyn>>) -> Result<Box<dyn CortexTrait + Send + Sync>> {
        let api_key = provider.api_key.clone();
        let model = provider.model_name.clone();

        // ✅ 根据 ModelCapability 构建对应的 Cortex
        // 职责边界清晰：ModelProvider 决定能力类型，Cortex 实现具体能力
        let cortex: Box<dyn CortexTrait + Send + Sync> = match provider.capability {
            // 🔷 Agent 类型 - 支持完整的对话、工具调用、向量化
            ModelCapability::Agent => match provider.provider_type {
                ProviderType::OpenAI => Box::new(
                    self::openai::OpenAiCortex::new(provider.id.clone(), api_key, model, provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::DeepSeek => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(provider.id.clone(), api_key, model, "https://api.deepseek.com".to_string(), provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::Qwen => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(provider.id.clone(), api_key, model, "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(), provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::Doubao => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(provider.id.clone(), api_key, model, "https://ark.cn-beijing.volces.com/api".to_string(), provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::Ollama => Box::new(
                    self::ollama::OllamaCortex::new(provider.id.clone(), api_key, model, provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::Custom => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(provider.id.clone(), api_key, model, provider.base_url.clone().unwrap_or_default(), None, rig_tools)?
                ),
                ProviderType::FastEmbed => {
                    return Err(anyhow::anyhow!("FastEmbed 仅支持 Embedding 能力，不支持 Agent 能力").into());
                }
            },
            // 🔷 Embedding 类型 - 只支持向量化，不需要构建完整的 Agent
            // 对于 Embedding 模型，直接复用 OpenAI 兼容实现（大多数 Embedding API 都是 OpenAI 格式）
            ModelCapability::Embedding => match provider.provider_type {
                ProviderType::OpenAI | ProviderType::DeepSeek | ProviderType::Qwen |
                ProviderType::Doubao | ProviderType::Ollama | ProviderType::Custom => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(
                        provider.id.clone(),
                        api_key, 
                        model, 
                        provider.base_url.clone().unwrap_or_default(), 
                        None, 
                        Vec::new()
                    )?
                ),
                ProviderType::FastEmbed => Box::new(
                    self::fastembed::FastEmbedCortex::new(
                        provider.id.as_str(),
                        model.as_str(),
                        provider.base_url.as_deref().unwrap_or(""),
                        provider.api_key.as_str(),
                    )?
                ),
            },
        };

        Ok(cortex)
    }

    async fn prompt(&self, _ctx: RequestContext, cortex: &dyn CortexTrait, prompt: &str) -> Result<String> {
        cortex.prompt(prompt).await
    }

    async fn embed_text_raw(&self, _ctx: RequestContext, cortex: &dyn CortexTrait, text: &str) -> Result<Vec<f32>> {
        let vectors = cortex.embeddings(&[text.to_string()]).await?;
        Ok(vectors.into_iter().next().unwrap_or_default())
    }

    async fn embed_entity(&self, _ctx: RequestContext, cortex: &dyn CortexTrait, entity: &dyn crate::models::vector::Vectorizable) -> Result<crate::models::vector::VectorIndexParams> {
        // ✅ 完整组装 VectorIndexParams - 从 CortexTrait 直接获取所有信息

        // 1. 获取待向量化的文本
        let text = entity.vectorize_text();

        // 2. 直接使用 cortex 的 embeddings 能力（Cortex 内部已经提前初始化好）
        let vectors = cortex.embeddings(&[text.clone()]).await?;
        let vector = vectors.into_iter().next().unwrap_or_default();

        // 3. 从 CortexTrait 直接获取元信息，不需要外部传入
        let params = crate::models::vector::VectorIndexParams {
            vector,
            content_hash: entity.vector_content_hash(),
            model_provider_id: cortex.model_provider_id().to_string(),
            embedding_model: cortex.model_name().to_string(),
            expire_at: entity.vector_expire_at(),
        };

        Ok(params)
    }

    async fn embed_text_for_search(&self, _ctx: RequestContext, cortex: &dyn CortexTrait, text: &str) -> Result<crate::models::vector::VectorIndexParams> {
        // 1. 直接使用 cortex 的 embeddings 能力
        let vectors = cortex.embeddings(&[text.to_string()]).await?;
        let vector = vectors.into_iter().next().unwrap_or_default();

        // 2. 计算文本内容哈希
        let content_hash = sha256::digest(text);

        // 3. 从 CortexTrait 直接获取元信息
        let params = crate::models::vector::VectorIndexParams {
            vector,
            content_hash,
            model_provider_id: cortex.model_provider_id().to_string(),
            embedding_model: cortex.model_name().to_string(),
            expire_at: None,
        };

        Ok(params)
    }
}

// 具体不同提供商的 Cortex 实现
pub mod openai;
pub mod openai_compatible;
pub mod ollama;
pub mod fastembed;
