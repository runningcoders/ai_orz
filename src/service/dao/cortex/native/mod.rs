//! Native Cortex DAO - 自建的模型调用实现（替代 rig）
//!
//! 提供新的 `CortexDao` trait（与 `crate::service::dao::cortex::CortexDao` 旧 trait 区分），
//! 按 provider_type 路由到具体实现。所有实现都是无状态单例（仅持有共享 reqwest::Client），
//! 所有配置从 `&ModelProviderPo` 读取。

use crate::models::cortex_types::{ChatMessage, ThinkResult, ToolDescriptor};
use crate::models::model_provider::{ModelProviderConfig, ModelProviderPo};
use crate::models::vector::{VectorIndexParams, VectorPayload, Vectorizable};
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::enums::{ChatApiFlavor, ProviderType};
use common::error::{Result, err};
use std::sync::{Arc, OnceLock};

pub mod http;
pub mod openai;
pub mod responses;

/// no-vision 降级：provider 不支持视觉输入时把 `UserMultimodal` 降级为纯文本
///
/// 判定源 = `ModelProviderConfig.supports_vision` 结构体字段（批5 单一事实源；
/// **缺省 None=false 保守降级**；config 解析失败按 false + 告警日志，不中断推理）。
/// 零迁移：存量 config JSON 缺字段反序列化为 None，自动兼容。
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

/// 读取 provider 的 vision 能力位（批5：改读 `ModelProviderConfig` 结构体字段，
/// 消除 raw JSON 键硬编码与「非 bool 静默 false」两类隐患，单一事实源）
///
/// 缺省 None / 解析失败 → false（保守降级，与存量纯文本链路行为一致；
/// 显式非 bool 值经 serde 反序列化报错，走解析失败分支，行为与批4 等价）。
fn provider_supports_vision(provider: &ModelProviderPo) -> bool {
    match serde_json::from_str::<ModelProviderConfig>(&provider.config) {
        Ok(cfg) => cfg.supports_vision_or_default(),
        Err(_) => {
            log_warn!(
                "cortex supports_vision probe: provider config parse failed, fallback to false, provider_id={}",
                provider.id
            );
            false
        }
    }
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
        options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<ThinkResult>;

    /// 文本转向量（原始向量）
    ///
    /// 默认实现显式失败：只有厂商实现（[`CortexDaoRegistry::embed_dao`] 选出的
    /// OpenAI 兼容 DAO）具备向量化能力。chat 协议实现（如 Responses DAO）不提供
    /// 该能力，也不应被路由到 —— 默认失败把「误路由」变成可见错误而非静默走错端点。
    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        Err(err!(
            UnsupportedOperation,
            "this CortexDao does not support embeddings; route via CortexDaoRegistry::embed_dao"
        ))
    }

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

/// 读取 provider 的 chat 协议口味（`config.api_flavor`）
///
/// 协议口味解析：config 正常 → `api_flavor_or_default()`（缺省 Responses）；
/// config **解析失败** → [`ChatApiFlavor::ChatCompletions`]。
///
/// 两条路径刻意不同：解析失败意味着这条记录已损坏、无从得知用户意图，
/// 此时选**覆盖面最广**的端点（`/chat/completions` 是 OpenAI 兼容网关的事实标准）
/// 比选较新的 `/responses` 更可能让推理继续跑通；且该分支有 `log_warn!` 告警，
/// 属显式降级路径，不是静默兜底。
fn provider_api_flavor(provider: &ModelProviderPo) -> ChatApiFlavor {
    match serde_json::from_str::<ModelProviderConfig>(&provider.config) {
        Ok(cfg) => cfg.api_flavor_or_default(),
        Err(e) => {
            log_warn!(
                "cortex api_flavor probe: provider config parse failed, fallback to chat_completions, provider_id={} error={}",
                provider.id,
                e
            );
            ChatApiFlavor::ChatCompletions
        }
    }
}

/// Cortex DAO 注册表
///
/// 路由按**调用面**分两个入口，而不是一条规则套所有方法：
/// - [`Self::chat_dao`]：`think()` 面，`config.api_flavor` 先定 wire 协议
///   （错了必然 400），`provider_type` 再定厂商/端点；
/// - [`Self::embed_dao`]：`embed()` 面，Embeddings 端点与 chat wire 协议无关，
///   `api_flavor` **不参与**路由，只看 `provider_type`。
///
/// 这条拆分修的是「配置了 Responses 协议的向量模型被错误路由到 Responses DAO」
/// 的历史缺陷（典型现象：方舟 vision embedding 走 `/embeddings` 报
/// 「该模型不能使用该 API」，而真正的 `/embeddings/multimodal` 分支不可达）。
/// 所有实现都是无状态单例。
pub struct CortexDaoRegistry {
    openai_compatible: Arc<openai::OpenAiCompatibleCortexDao>,
    responses: Arc<responses::ResponsesCortexDao>,
    // fastembed 和 external 在后续 Task 6 中添加
}

impl CortexDaoRegistry {
    fn new() -> Self {
        Self {
            openai_compatible: Arc::new(openai::OpenAiCompatibleCortexDao::new()),
            responses: Arc::new(responses::ResponsesCortexDao::new()),
        }
    }

    /// 获取思考（`think()`）面 DAO
    ///
    /// 入参是完整的 `&ModelProviderPo` 而非 `ProviderType`：协议口味
    /// (`config.api_flavor`) 与厂商 (`provider_type`) 是**正交两维**，
    /// 前者只能从 config 读。
    pub fn chat_dao(&self, provider: &ModelProviderPo) -> Arc<dyn CortexDao> {
        match provider_api_flavor(provider) {
            ChatApiFlavor::Responses => self.responses.clone(),
            ChatApiFlavor::ChatCompletions => self.provider_type_dao(provider.provider_type),
        }
    }

    /// 获取向量化（`embed()`）面 DAO
    ///
    /// **刻意忽略 `config.api_flavor`**：`/embeddings` 是独立端点，与 chat 走
    /// `/chat/completions` 还是 `/responses` 无关。若沿用 chat 的协议口味来选 DAO，
    /// 配了 Responses 的向量模型会被路由到 Responses DAO —— 它不提供向量化能力，
    /// 且 DoubaoVision 的 `/embeddings/multimodal` 分支永远不可达。
    pub fn embed_dao(&self, provider: &ModelProviderPo) -> Arc<dyn CortexDao> {
        self.provider_type_dao(provider.provider_type)
    }

    /// 按厂商路由到具体实现（chat_completions 与 embeddings 共用同一张厂商表）
    fn provider_type_dao(&self, provider_type: ProviderType) -> Arc<dyn CortexDao> {
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
            // jev（System One 决策模型）不兼容 OpenAI 协议，不属于 cortex 路由；
            // 一期未接运行时，正常业务不会走到此分支。误用时显式报错而非静默
            // fallback（防 FastEmbed「枚举已挂、实现悬空」反例扩散到 Jev）。
            ProviderType::Jev => {
                panic!(
                    "Jev/System One is a cerebellum decision model, not a cortex chat/embedding provider; use dao::cerebellum::dao() for fast decisions"
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

    /// 批5：probe 改读结构体字段后行为等价断言——set_config 结构体字段 true
    /// 与 raw JSON 显式 true 两条写入路径均透传（单一事实源双向可写）
    #[test]
    fn supports_vision_via_struct_field_passes_through() {
        let mut provider = provider_with_config("{}");
        provider.update_config(|cfg| cfg.supports_vision = Some(true));
        let out = downgrade_messages_for_no_vision(&[multimodal()], &provider);
        assert!(matches!(out[0], ChatMessage::UserMultimodal { .. }));
    }

    /// 批5：显式 false 与非 bool 值（serde Err → 解析失败分支）均保守降级，
    /// 与批4 as_bool 失败行为等价
    #[test]
    fn explicit_false_and_non_bool_value_both_downgrade() {
        let explicit_false = provider_with_config(r#"{"supports_vision": false}"#);
        let out = downgrade_messages_for_no_vision(&[multimodal()], &explicit_false);
        assert!(matches!(out[0], ChatMessage::User { .. }));

        let non_bool = provider_with_config(r#"{"supports_vision": "yes"}"#);
        let out = downgrade_messages_for_no_vision(&[multimodal()], &non_bool);
        assert!(matches!(out[0], ChatMessage::User { .. }));
    }
}
