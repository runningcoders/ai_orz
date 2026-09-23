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
}

impl fmt::Display for ModelCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelCapability::Agent => write!(f, "agent"),
            ModelCapability::Embedding => write!(f, "embedding"),
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
