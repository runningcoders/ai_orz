//! 模型提供商类型枚举

use serde::{Deserialize, Serialize};

/// 模型提供商类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    /// OpenAI 官方
    OpenAI,
    /// DeepSeek
    DeepSeek,
    /// 阿里云通义千问
    Qwen,
    /// 字节跳动豆包
    Doubao,
    /// 本地 Ollama
    Ollama,
    /// 其他 OpenAI 兼容接口
    OpenAICompatible,
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::OpenAI
    }
}
