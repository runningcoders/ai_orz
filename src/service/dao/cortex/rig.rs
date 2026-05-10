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
                    self::openai::OpenAiCortex::new(api_key, model, provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::DeepSeek => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(api_key, model, "https://api.deepseek.com".to_string(),  provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::Qwen => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(api_key, model, "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),  provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::Doubao => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(api_key, model, "https://ark.cn-beijing.volces.com/api".to_string(),  provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::Ollama => Box::new(
                    self::ollama::OllamaCortex::new(api_key, model,  provider.base_url.clone(), rig_tools)?
                ),
                ProviderType::Custom => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(api_key, model, provider.base_url.clone().unwrap_or_default(), None, rig_tools)?
                ),
            },
            // 🔷 Embedding 类型 - 只支持向量化，不需要构建完整的 Agent
            // 对于 Embedding 模型，直接复用 OpenAI 兼容实现（大多数 Embedding API 都是 OpenAI 格式）
            ModelCapability::Embedding => match provider.provider_type {
                ProviderType::OpenAI | ProviderType::DeepSeek | ProviderType::Qwen |
                ProviderType::Doubao | ProviderType::Ollama | ProviderType::Custom => Box::new(
                    self::openai_compatible::OpenAiCompatibleCortex::new(
                        api_key, 
                        model, 
                        provider.base_url.clone().unwrap_or_default(), 
                        None, 
                        Vec::new()
                    )?
                ),
            },
        };

        Ok(cortex)
    }

    async fn prompt(&self, _ctx: RequestContext, cortex: &dyn CortexTrait, prompt: &str) -> Result<String> {
        cortex.prompt(prompt).await
    }
}

// ==================== 固有方法（不放在 trait 里，避免 dyn 不兼容 ====================

use super::CortexDao;

impl RigCortexDao {
    /// ✅ 实体向量化（用于索引场景，输入业务实体）
    pub async fn embed<T: crate::models::vector::Vectorizable + Send + Sync>(
        &self,
        ctx: RequestContext,
        provider: &crate::models::model_provider::ModelProvider,
        entity: &T,
    ) -> Result<VectorIndexParams> {
        // ✅ 完整组装 VectorIndexParams - 由 CortexDao 全权负责，上层不需要关心

        // 1. 获取待向量化的文本
        let text = entity.vectorize_text();

        // 2. 创建临时 Cortex（不需要工具）并调用真实 embedding 能力
        let cortex = self.create_cortex_trait(ctx, &provider.po, Vec::new())?;
        let vectors = cortex.embeddings(&[text.clone()]).await?;
        let vector = vectors.into_iter().next().unwrap_or_default();

        // 3. 组装完整参数（CortexDao 拥有所有信息）
        let params = VectorIndexParams {
            vector,
            content_hash: entity.vector_content_hash(),
            model_provider_id: provider.po.id.clone(),
            embedding_model: provider.po.model_name.clone(),
            expire_at: entity.vector_expire_at(),
        };

        Ok(params)
    }

    /// ✅ 文本直接向量化（用于搜索场景，输入用户查询文本）
    pub async fn embed_text(
        &self,
        ctx: RequestContext,
        provider: &crate::models::model_provider::ModelProvider,
        text: &str,
    ) -> Result<VectorIndexParams> {
        // 1. 创建临时 Cortex（不需要工具）并调用真实 embedding 能力
        let cortex = self.create_cortex_trait(ctx, &provider.po, Vec::new())?;
        let vectors = cortex.embeddings(&[text.to_string()]).await?;
        let vector = vectors.into_iter().next().unwrap_or_default();

        // 2. 计算文本内容哈希（用于搜索对比）
        let content_hash = sha256::digest(text);

        // 3. 组装完整参数
        let params = VectorIndexParams {
            vector,
            content_hash,
            model_provider_id: provider.po.id.clone(),
            embedding_model: provider.po.model_name.clone(),
            expire_at: None, // 搜索不需要过期
        };

        Ok(params)
    }
}

// 具体不同提供商的 Cortex 实现
pub mod openai;
pub mod openai_compatible;
pub mod ollama;
