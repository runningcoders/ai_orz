//! Brain 实体 - 封装完整的思考执行环境
//!
//! Brain 直接持有 ModelProvider 和 CortexTrait，逻辑关系更直观：
//! - Brain 包含 ModelProvider（配置信息）
//! - Brain 包含 Cortex（推理执行实例）
//! - ModelProvider 只保留配置信息，不持有执行实例

use crate::models::model_provider::ModelProvider;
use async_trait::async_trait;
use anyhow::Result;

/// 统一的 CortexTrait - 大脑皮层，负责思考推理
#[async_trait]
pub trait CortexTrait: Send + Sync {
    /// 运行 prompt，获取回答
    async fn prompt(&self, prompt: &str) -> Result<String>;

    /// 是否支持工具调用
    fn support_tools(&self) -> bool;
}

/// Brain 封装了完整的思考执行环境
///
/// Brain 直接持有 ModelProvider 和 Cortex
/// - ModelProvider 提供配置信息
/// - Cortex 提供推理执行能力
pub struct Brain {
    /// 关联的模型提供商（业务对象，包含配置信息）
    pub model_provider: ModelProvider,
    /// 思考推理执行 cortex
    pub cortex: Box<dyn CortexTrait + Send + Sync>,
}

impl Brain {
    /// 创建新 Brain
    pub fn new(model_provider: ModelProvider, cortex: Box<dyn CortexTrait + Send + Sync>) -> Self {
        Self {
            model_provider,
            cortex,
        }
    }

    /// 获取 Cortex 引用
    pub fn cortex(&self) -> &(dyn CortexTrait + Send + Sync) {
        &*self.cortex
    }
}
