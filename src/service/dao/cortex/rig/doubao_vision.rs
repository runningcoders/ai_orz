//! 豆包 Vision Embedding Cortex 实现
//!
//! 对应 `ProviderType::DoubaoVision`：豆包的 vision embedding 模型（如
//! `doubao-embedding-vision-251215`）支持纯文本与图文的向量化，但使用的是
//! 火山方舟的 `/embeddings/multimodal` endpoint，与标准 OpenAI `/embeddings`
//! 不兼容：
//! - 请求体：`input` 是对象数组 `[{type:"text", text:"..."}]` 而非字符串数组
//! - 响应体：`data.embedding`（单对象）而非 `data[0].embedding`（数组元素）
//! - 多文本语义：multimodal endpoint 把 input 数组视为一个多模态内容的组合，
//!   会融合成一个 embedding；纯文本批量场景必须逐条请求。
//!
//! 本实现仅负责向量化（ModelCapability::Embedding），不支持 prompt。

use super::*;
use anyhow::anyhow;
use async_trait::async_trait;
use common::enums::ModelCapability;
use common::error::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 豆包 Vision Embedding Cortex
///
/// 通过 reqwest 直接调用 `/embeddings/multimodal`，绕过 rig 的 `EmbeddingModel`
/// trait（rig 硬编码调用标准 `/embeddings`，不兼容多模态 endpoint）。
#[derive(Clone)]
pub struct DoubaoVisionCortex {
    client: reqwest::Client,
    base_url: String,
    api_key: Arc<str>,
    model_provider_id: Arc<str>,
    model_name: Arc<str>,
}

impl std::fmt::Debug for DoubaoVisionCortex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DoubaoVisionCortex")
            .field("model_provider_id", &self.model_provider_id)
            .field("model_name", &self.model_name)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl DoubaoVisionCortex {
    /// 创建新的 DoubaoVisionCortex
    ///
    /// `base_url` 应为火山方舟 API 根地址（如 `https://ark.cn-beijing.volces.com/api/v3`）。
    /// 传入时已剥离尾部 `/`，调用时拼接 `/embeddings/multimodal`。
    pub fn new(
        model_provider_id: String,
        api_key: String,
        model: String,
        base_url: String,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow!("Failed to build reqwest client: {}", e))?;

        // 规范化 base_url：去掉尾部 '/'
        let base_url = base_url.trim_end_matches('/').to_string();

        Ok(Self {
            client,
            base_url,
            api_key: Arc::from(api_key.as_str()),
            model_provider_id: Arc::from(model_provider_id.as_str()),
            model_name: Arc::from(model.as_str()),
        })
    }

    /// 调用 `/embeddings/multimodal` 对单条文本生成 embedding
    ///
    /// multimodal endpoint 的 input 数组在多文本场景下会融合成一个 embedding，
    /// 因此批量向量化在外层 `embeddings()` 中逐条调用本方法。
    async fn embed_single_text(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let url = format!("{}/embeddings/multimodal", self.base_url);
        let body = EmbeddingRequest {
            model: self.model_name.as_ref(),
            input: vec![EmbeddingInput { kind: "text", text }],
        };

        let resp = self
            .client
            .post(&url)
            .bearer_auth(self.api_key.as_ref())
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("DoubaoVision embedding request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "<no body>".to_string());
            return Err(anyhow!(
                "DoubaoVision embedding request failed ({}): {}",
                status,
                text
            ));
        }

        let resp_body: EmbeddingResponse = resp
            .json()
            .await
            .map_err(|e| anyhow!("DoubaoVision embedding response parse failed: {}", e))?;

        Ok(resp_body
            .data
            .embedding
            .into_iter()
            .map(|x| x as f32)
            .collect())
    }
}

// ==================== 请求/响应结构 ====================

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: Vec<EmbeddingInput<'a>>,
}

#[derive(Serialize)]
struct EmbeddingInput<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    text: &'a str,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: EmbeddingData,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f64>,
}

// ==================== CortexTrait 实现 ====================

#[async_trait]
impl CortexTrait for DoubaoVisionCortex {
    fn capability(&self) -> ModelCapability {
        ModelCapability::Embedding
    }

    fn model_provider_id(&self) -> &str {
        &self.model_provider_id
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn prompt(&self, _prompt: &str) -> anyhow::Result<String> {
        Err(anyhow!(
            "DoubaoVisionCortex 仅支持向量化，不支持 prompt 功能"
        ))
    }

    async fn embeddings(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        // multimodal endpoint 在多文本场景下会融合成一个 embedding，
        // 纯文本批量向量化必须逐条请求。
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            let vec = self.embed_single_text(text).await?;
            results.push(vec);
        }
        Ok(results)
    }

    fn support_tools(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_url_normalization() {
        // 尾部 '/' 被剥离
        let cortex = DoubaoVisionCortex::new(
            "provider-1".to_string(),
            "key".to_string(),
            "doubao-embedding-vision-251215".to_string(),
            "https://ark.cn-beijing.volces.com/api/v3/".to_string(),
        )
        .unwrap();
        assert_eq!(cortex.base_url, "https://ark.cn-beijing.volces.com/api/v3");

        // 无尾部 '/' 不变
        let cortex = DoubaoVisionCortex::new(
            "provider-1".to_string(),
            "key".to_string(),
            "doubao-embedding-vision-251215".to_string(),
            "https://ark.cn-beijing.volces.com/api/v3".to_string(),
        )
        .unwrap();
        assert_eq!(cortex.base_url, "https://ark.cn-beijing.volces.com/api/v3");
    }
}
