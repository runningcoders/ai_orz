//! Brain 实体和 Cortex 实体
//!
//! 最终结构：
//! - Brain 直接持有 Cortex 实体
//! - Cortex 实体持有 ModelProvider 和 CortexTrait（推理执行）
//! - ModelProvider 只保存配置信息

use crate::models::model_provider::ModelProvider;
use async_trait::async_trait;
use anyhow::Result;

/// 统一的 CortexTrait - 大脑皮层 trait，定义推理接口
#[async_trait]
pub trait CortexTrait: Send + Sync {
    /// 运行 prompt，获取回答
    async fn prompt(&self, prompt: &str) -> Result<String>;

    /// 是否支持工具调用
    fn support_tools(&self) -> bool;
}

/// Cortex 实体 - 持有 ModelProvider 和具体的推理实现
///
/// Cortex = 模型配置 + 推理执行
pub struct Cortex {
    /// 关联的模型提供商（业务对象，包含配置信息）
    pub model_provider: ModelProvider,
    /// 推理执行实例
    pub cortex: Box<dyn CortexTrait + Send + Sync>,
}

impl Cortex {
    /// 创建新 Cortex
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

/// Brain 封装了完整的思考执行环境
///
/// Brain 直接持有 Cortex 实体
pub struct Brain {
    /// Cortex 实体（包含模型配置 + 推理执行）
    pub cortex: Cortex,
}

impl Brain {
    /// 创建新 Brain
    pub fn new(cortex: Cortex) -> Self {
        Self { cortex }
    }

    /// 获取 Cortex 引用
    pub fn cortex(&self) -> &Cortex {
        &self.cortex
    }

    /// 获取 Cortex 内部的推理执行引用
    pub fn cortex_trait(&self) -> &(dyn CortexTrait + Send + Sync) {
        self.cortex.cortex()
    }
}
