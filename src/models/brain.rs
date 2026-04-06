//! Brain 实体 - 封装完整的思考执行环境
//!
//! Brain 持有 ModelProvider，ModelProvider 已经包含 Cortex，所以 Brain 直接通过 ModelProvider 获取 Cortex

use crate::models::model_provider::ModelProvider;
use async_trait::async_trait;
use anyhow::Result;

/// 统一的 Cortex trait - 大脑皮层，负责思考推理
#[async_trait]
pub trait Cortex: Send + Sync {
    /// 运行 prompt，获取回答
    async fn prompt(&self, prompt: &str) -> Result<String>;

    /// 是否支持工具调用
    fn support_tools(&self) -> bool;
}

/// Brain 封装了完整的思考执行环境
///
/// Brain 直接持有 ModelProvider，ModelProvider 已经包含 Cortex
/// 通过 ModelProvider 可以获取到 Cortex
pub struct Brain {
    /// 关联的模型提供商（业务对象，已包含 cortex）
    pub model_provider: ModelProvider,
}

impl Brain {
    /// 创建新 Brain
    pub fn new(model_provider: ModelProvider) -> Self {
        Self { model_provider }
    }

    /// 获取 Cortex 引用
    pub fn cortex(&self) -> Option<&(dyn Cortex + Send + Sync)> {
        self.model_provider.cortex()
    }
}
