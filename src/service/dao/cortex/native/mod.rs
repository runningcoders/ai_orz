//! Native Cortex DAO - 自建的模型调用实现（替代 rig）
//!
//! 提供新的 `CortexDao` trait（与 `crate::service::dao::cortex::CortexDao` 旧 trait 区分），
//! 按 provider_type 路由到具体实现。所有实现都是无状态单例（仅持有共享 reqwest::Client），
//! 所有配置从 `&ModelProviderPo` 读取。

use crate::models::cortex_types::{ChatMessage, ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::models::vector::{VectorIndexParams, VectorPayload, Vectorizable};
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::enums::ProviderType;
use common::error::Result;
use std::sync::{Arc, OnceLock};

pub mod http;
pub mod openai;

/// no-vision 降级：provider 不支持视觉输入时把 `UserMultimodal` 降级为纯文本
///
/// 判定源 = `provider.config` JSON 的 `supports_vision` 字段（**缺省 false 保守降级**；
/// config 解析失败按 false + 告警日志，不中断推理）。零迁移：config 是自由 JSON
/// 字符串，界面/接口可直接配置，无需改 `ModelProviderConfig` 结构。
///
/// 降级行为：清空 images，text 追加占位行说明图片未展示（模型仍能感知有附件）。
/// 支持 vision 的 provider 原样透传（零克隆降级路径直接 to_vec）。
pub fn downgrade_messages_for_no_vision(
    messages: &[ChatMessage],
    provider: &ModelProviderPo,
) -> Vec<ChatMessage> {
    if provider_supports_vision(provider) {
        return messages.to_vec();
    }
    messages
        .iter()
        .map(|m| match m {
            ChatMessage::UserMultimodal { text, images } => {
                let mut downgraded = text.clone();
                if !images.is_empty() {
                    downgraded.push_str(&format!(
                        "\n\n[图片附件：共 {} 张未展示：当前模型不支持视觉输入]",
                        images.len()
                    ));
                }
                ChatMessage::user(downgraded)
            }
            other => other.clone(),
        })
        .collect()
}

/// 读取 provider.config JSON 的 `supports_vision` 能力位
///
/// 缺省 / 非 bool / 解析失败 → false（保守降级，与存量纯文本链路行为一致）。
fn provider_supports_vision(provider: &ModelProviderPo) -> bool {
    let parsed: Option<serde_json::Value> = match serde_json::from_str(&provider.config) {
        Ok(v) => Some(v),
        Err(_) => {
            log_warn!(
                "cortex supports_vision probe: provider config parse failed, fallback to false, provider_id={}",
                provider.id
            );
            None
        }
    };
    parsed
        .as_ref()
        .and_then(|cfg| cfg.get("supports_vision"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

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
        let payload = entity.vector_payload();
        let payload_hash = payload.hash();
        Ok(VectorIndexParams {
            vector,
            content_hash: entity.vector_content_hash(),
            payload,
            payload_hash,
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
            // 搜索场景无实体，payload 为空（过滤条件由搜索侧谓词承担）
            payload: VectorPayload::default(),
            payload_hash: VectorPayload::default().hash(),
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
            // jev（System One 决策模型）不兼容 OpenAI 协议，不属于 cortex chat 路由；
            // 一期未接运行时，正常业务不会走到此分支。误用时显式报错而非静默
            // fallback（防 FastEmbed「枚举已挂、实现悬空」反例扩散到 Jev）。
            ProviderType::Jev => {
                panic!(
                    "Jev/System One is a cerebellum decision model, not a cortex chat provider; use dao::cerebellum::dao() for fast decisions"
                )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::cortex_types::ImagePart;
    use common::enums::{ModelCapability, ModelProviderStatus, ProviderType};

    fn provider_with_config(config: &str) -> ModelProviderPo {
        ModelProviderPo {
            id: "p1".to_string(),
            name: "p1".to_string(),
            provider_type: ProviderType::Custom,
            model_name: "m".to_string(),
            capability: ModelCapability::Agent,
            api_key: String::new(),
            base_url: None,
            description: None,
            config: config.to_string(),
            status: ModelProviderStatus::Normal,
            created_by: "system".to_string(),
            modified_by: "system".to_string(),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn multimodal() -> ChatMessage {
        ChatMessage::user_multimodal(
            "看图",
            vec![ImagePart {
                mime_type: "image/png".to_string(),
                data_base64: "AAAA".to_string(),
            }],
        )
    }

    #[test]
    fn default_config_downgrades_to_plain_user_with_placeholder() {
        let provider = provider_with_config("{}");
        let out = downgrade_messages_for_no_vision(&[multimodal()], &provider);
        match &out[0] {
            ChatMessage::User { content } => {
                assert!(content.starts_with("看图"));
                assert!(content.contains("[图片附件：共 1 张未展示：当前模型不支持视觉输入]"));
            }
            other => panic!("期望降级为纯文本 User，实际 {:?}", other),
        }
    }

    #[test]
    fn supports_vision_true_passes_through() {
        let provider = provider_with_config(r#"{"supports_vision": true}"#);
        let out = downgrade_messages_for_no_vision(&[multimodal()], &provider);
        assert!(matches!(out[0], ChatMessage::UserMultimodal { .. }));
    }

    #[test]
    fn dirty_config_falls_back_to_false_and_downgrades() {
        let provider = provider_with_config("not-json");
        let out = downgrade_messages_for_no_vision(&[multimodal()], &provider);
        assert!(matches!(out[0], ChatMessage::User { .. }));
    }

    #[test]
    fn plain_user_messages_untouched() {
        let provider = provider_with_config("{}");
        let out = downgrade_messages_for_no_vision(&[ChatMessage::user("hi")], &provider);
        match &out[0] {
            ChatMessage::User { content } => assert_eq!(content, "hi"),
            other => panic!("纯文本消息不得被降级改动，实际 {:?}", other),
        }
    }
}
