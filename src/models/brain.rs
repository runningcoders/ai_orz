//! Brain 实体 - 封装 cortex 思考模块
//!
//! Brain 持有 ModelProvider 和 Cortex，是完整的思考执行实体

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
/// 由上层根据 ModelProvider 配置创建
/// Brain 直接持有 ModelProvider 和 Cortex，方便后续统计和扩展
pub struct Brain {
    /// 关联的模型提供商（业务对象，可能已装配 cortex）
    pub model_provider: ModelProvider,
    /// 思考推理执行 cortex
    pub(crate) cortex: Box<dyn Cortex + Send + Sync>,
}

impl Brain {
    /// 创建新 Brain
    pub fn new(model_provider: ModelProvider, cortex: Box<dyn Cortex + Send + Sync>) -> Self {
        Self {
            model_provider,
            cortex,
        }
    }
}
