//! Model provider related enums

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;
use std::fmt;

/// Model provider type
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ProviderType {
    /// OpenAI compatible
    #[default]
    OpenAI = 0,
    /// DeepSeek
    DeepSeek = 1,
    /// 通义千问
    Qwen = 2,
    /// 豆包
    Doubao = 3,
    /// Ollama local
    Ollama = 4,
    /// Custom OpenAI compatible
    Custom = 5,
    /// FastEmbed local embedding (纯本地向量化，无外部依赖)
    FastEmbed = 6,
    /// 豆包 Vision 多模态 Embedding（使用 /embeddings/multimodal endpoint）
    DoubaoVision = 7,
    /// jev（TypeSafe AI System One 决策模型，小脑 cerebellum）
    ///
    /// 协议维度：System One 形态（state + 类型化问题 → 带概率决策），
    /// 不兼容 OpenAI chat/completions，走 dao/cerebellum 独立 System One client。
    /// #[repr(i32)] 尾部追加变体，免 DB migration（仓库既有模式）。
    Jev = 8,
}

/// Model capability type - 区分模型是用于 Agent 思考还是 Embedding 向量化
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ModelCapability {
    /// Agent 类型 - 支持对话、思考、工具调用
    #[default]
    Agent = 0,
    /// Embedding 类型 - 支持向量化
    Embedding = 1,
    /// Decision 类型 - 小脑快判断（cerebellum，System One 决策模型）
    ///
    /// 用途维度：标记「决策/快判断」用途，区别于 Agent（对话思考）与
    /// Embedding（向量化）。调度与展示按用途过滤（T1 报告 §3.2）。
    Decision = 2,
}

impl From<i32> for ProviderType {
    fn from(v: i32) -> Self {
        match v {
            0 => ProviderType::OpenAI,
            1 => ProviderType::DeepSeek,
            2 => ProviderType::Qwen,
            3 => ProviderType::Doubao,
            4 => ProviderType::Ollama,
            5 => ProviderType::Custom,
            6 => ProviderType::FastEmbed,
            7 => ProviderType::DoubaoVision,
            8 => ProviderType::Jev,
            _ => ProviderType::default(),
        }
    }
}

impl ProviderType {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderType::OpenAI => write!(f, "openai"),
            ProviderType::DeepSeek => write!(f, "deepseek"),
            ProviderType::Qwen => write!(f, "qwen"),
            ProviderType::Doubao => write!(f, "doubao"),
            ProviderType::Ollama => write!(f, "ollama"),
            ProviderType::Custom => write!(f, "custom"),
            ProviderType::FastEmbed => write!(f, "fastembed"),
            ProviderType::DoubaoVision => write!(f, "doubao_vision"),
            ProviderType::Jev => write!(f, "jev"),
        }
    }
}

impl From<ProviderType> for i32 {
    fn from(t: ProviderType) -> i32 {
        t as i32
    }
}

impl From<i64> for ProviderType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl From<i32> for ModelCapability {
    fn from(v: i32) -> Self {
        match v {
            0 => ModelCapability::Agent,
            1 => ModelCapability::Embedding,
            2 => ModelCapability::Decision,
            _ => ModelCapability::default(),
        }
    }
}

impl ModelCapability {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }

    /// Check if it's Agent capability
    pub fn is_agent(&self) -> bool {
        matches!(self, ModelCapability::Agent)
    }

    /// Check if it's Embedding capability
    pub fn is_embedding(&self) -> bool {
        matches!(self, ModelCapability::Embedding)
    }

    /// Check if it's Decision capability（小脑快判断）
    pub fn is_decision(&self) -> bool {
        matches!(self, ModelCapability::Decision)
    }
}

impl fmt::Display for ModelCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelCapability::Agent => write!(f, "agent"),
            ModelCapability::Embedding => write!(f, "embedding"),
            ModelCapability::Decision => write!(f, "decision"),
        }
    }
}

impl From<ModelCapability> for i32 {
    fn from(t: ModelCapability) -> i32 {
        t as i32
    }
}

impl From<i64> for ModelCapability {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// 模型下行调用访问模式（model_provider config JSON 内可选字段，非 DB 独立列）
///
/// 平台默认以 stream 模式调用下游网关；部分下游网关不支持 stream，
/// 可配置为 NonStream 走非流式调用。缺省（未配置/脏配置兜底）恒为 Stream
/// = 平台历史行为，存量配置零影响。
///
/// serde 线上取值契约：`"stream"` / `"non_stream"`（snake_case；
/// `non_stream` 不带连字符 —— 方案 §2.1 定稿取值，见 access_mode_serde_roundtrip 单测锁）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelAccessMode {
    /// 流式调用（平台现状默认）
    #[default]
    Stream,
    /// 非流式调用（下游网关不支持 stream 时的兼容模式）
    NonStream,
}

/// Chat 协议口味（model_provider config JSON 内可选字段，非 DB 独立列）
///
/// 维度正交于 [`ProviderType`]：ProviderType 回答「哪家厂商/哪个端点」，
/// api_flavor 回答「用哪种 wire 协议」。同一厂商可能同时提供两种协议
/// （OpenAI 自家即如此），故不可把协议塞进 ProviderType（否则 Custom 类型
/// 枚举爆炸）。先例：`ModelProviderConfig::supports_vision` 注释已确立
/// 「与 provider_type 相互独立」原则。
///
/// 缺省（未配置/脏配置兜底）恒为 Responses = **平台主线协议**。
///
/// ⚠️ 语义变更（2026-10-21）：缺省由 `ChatCompletions` 改为 `Responses`。
/// 取舍：Responses 是能力主线（方舟的 MCP / 知识库 / 联网搜索 / 上下文缓存
/// 仅此协议提供），且主流实现均已覆盖（方舟、OpenAI、Ollama v0.13.3+）。
/// 未提供 `/responses` 端点的自建/老旧网关由用户**显式**配置
/// `api_flavor: "chat_completions"` 回退——不做按 provider_type 的特判。
///
/// serde 线上取值契约：`"chat_completions"` / `"responses"`（snake_case）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChatApiFlavor {
    /// 传统 `/chat/completions` 协议（兼容回退选项，需显式配置）
    ChatCompletions,
    /// OpenAI Responses API（`/responses`）：input item 数组 + 语义事件流。
    ///
    /// 平台主线协议，也是 `Default`。
    ///
    /// 与 ChatCompletions 的差异不在传输形态（两者都支持 stream/non_stream，
    /// 由 `access_mode` 独立控制），而在请求体结构与 SSE 事件模型，故实现
    /// 拆分为独立的 `ResponsesCortexDao`。
    #[default]
    Responses,
}

#[cfg(test)]
mod access_mode_tests {
    use super::*;

    #[test]
    fn access_mode_default_is_stream() {
        assert_eq!(ModelAccessMode::default(), ModelAccessMode::Stream);
    }

    #[test]
    fn access_mode_serde_roundtrip() {
        // 锁定线上取值契约：stream / non_stream（snake_case）
        assert_eq!(
            serde_json::to_string(&ModelAccessMode::Stream).unwrap(),
            "\"stream\""
        );
        assert_eq!(
            serde_json::to_string(&ModelAccessMode::NonStream).unwrap(),
            "\"non_stream\""
        );
        let s: ModelAccessMode = serde_json::from_str("\"stream\"").unwrap();
        assert_eq!(s, ModelAccessMode::Stream);
        let ns: ModelAccessMode = serde_json::from_str("\"non_stream\"").unwrap();
        assert_eq!(ns, ModelAccessMode::NonStream);
    }
}

#[cfg(test)]
mod api_flavor_tests {
    use super::*;

    #[test]
    fn api_flavor_default_is_responses() {
        // 主线协议：未配置即 Responses（2026-10-21 语义变更，见枚举定义注释）
        assert_eq!(ChatApiFlavor::default(), ChatApiFlavor::Responses);
    }

    #[test]
    fn api_flavor_serde_roundtrip() {
        // 锁定线上取值契约：chat_completions / responses（snake_case）
        assert_eq!(
            serde_json::to_string(&ChatApiFlavor::ChatCompletions).unwrap(),
            "\"chat_completions\""
        );
        assert_eq!(
            serde_json::to_string(&ChatApiFlavor::Responses).unwrap(),
            "\"responses\""
        );
        let cc: ChatApiFlavor = serde_json::from_str("\"chat_completions\"").unwrap();
        assert_eq!(cc, ChatApiFlavor::ChatCompletions);
        let r: ChatApiFlavor = serde_json::from_str("\"responses\"").unwrap();
        assert_eq!(r, ChatApiFlavor::Responses);
    }
}

#[cfg(test)]
mod cerebellum_variant_tests {
    use super::*;

    #[test]
    fn jev_variant_roundtrip() {
        assert_eq!(ProviderType::Jev.to_i32(), 8);
        assert_eq!(ProviderType::from_i32(8), ProviderType::Jev);
        assert_eq!(ProviderType::from(8_i64), ProviderType::Jev);
        // 尾部追加不破坏既有变体
        assert_eq!(ProviderType::from_i32(7), ProviderType::DoubaoVision);
        assert_eq!(ProviderType::from_i32(0), ProviderType::OpenAI);
        // 未知值兜底 default，不 panic
        assert_eq!(ProviderType::from_i32(9), ProviderType::default());
        assert_eq!(ProviderType::Jev.to_string(), "jev");
    }

    #[test]
    fn decision_capability_roundtrip() {
        assert_eq!(ModelCapability::Decision.to_i32(), 2);
        assert_eq!(ModelCapability::from_i32(2), ModelCapability::Decision);
        assert!(ModelCapability::Decision.is_decision());
        assert!(!ModelCapability::Agent.is_decision());
        assert!(!ModelCapability::Embedding.is_decision());
        // 既有变体不漂移
        assert_eq!(ModelCapability::from_i32(0), ModelCapability::Agent);
        assert_eq!(ModelCapability::from_i32(1), ModelCapability::Embedding);
        assert_eq!(ModelCapability::Decision.to_string(), "decision");
    }

    #[test]
    fn decision_capability_serde_roundtrip() {
        let s = serde_json::to_string(&ModelCapability::Decision).unwrap();
        let back: ModelCapability = serde_json::from_str(&s).unwrap();
        assert_eq!(back, ModelCapability::Decision);
    }
}
