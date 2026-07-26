//! FastEmbed 本地向量化 Cortex 实现

use super::*;
use ::fastembed::{InitOptions, TextEmbedding};
use anyhow::anyhow;
use async_trait::async_trait;
use common::enums::ModelCapability;
use common::error::Result;
use std::sync::Arc;
use std::sync::Mutex;

/// FastEmbed Cortex 实现
pub struct FastEmbedCortex {
    model_provider_id: Arc<str>,
    model_name: Arc<str>,
    embedding: Arc<Mutex<TextEmbedding>>,
}

impl std::fmt::Debug for FastEmbedCortex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastEmbedCortex")
            .field("model_provider_id", &self.model_provider_id)
            .field("model_name", &self.model_name)
            .finish()
    }
}

impl Clone for FastEmbedCortex {
    fn clone(&self) -> Self {
        Self {
            model_provider_id: self.model_provider_id.clone(),
            model_name: self.model_name.clone(),
            embedding: self.embedding.clone(),
        }
    }
}

impl FastEmbedCortex {
    /// 创建新的 FastEmbed Cortex
    pub fn new(
        model_provider_id: &str,
        model_name: &str,
        _base_url: &str,
        _api_key: &str,
    ) -> Result<Self> {
        // 使用默认模型 fast-bge-small-en
        // 中文模型需要 fastembed 的 nomic 或者其他特性
        let embedding = TextEmbedding::try_new(InitOptions::default())?;

        Ok(Self {
            model_provider_id: Arc::from(model_provider_id),
            model_name: Arc::from(model_name),
            embedding: Arc::new(Mutex::new(embedding)),
        })
    }
}

#[async_trait]
impl CortexTrait for FastEmbedCortex {
    async fn prompt(&self, _prompt: &str) -> anyhow::Result<String> {
        Err(anyhow!("FastEmbed 仅支持向量化，不支持 prompt 功能"))
    }

    async fn embeddings(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        // fastembed 的 embed 是同步的，用 spawn_blocking 包装
        let texts = texts.to_vec();
        let embedding = self.embedding.clone();

        let embeddings = tokio::task::spawn_blocking(move || {
            let mut embed = embedding.lock().map_err(|e| anyhow!("锁失败: {}", e))?;
            (*embed).embed(texts, None)
        })
        .await??;

        Ok(embeddings)
    }

    fn model_provider_id(&self) -> &str {
        &self.model_provider_id
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn capability(&self) -> ModelCapability {
        ModelCapability::Embedding
    }

    fn support_tools(&self) -> bool {
        false
    }
}
