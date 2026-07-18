//! Brain 实体和 Cortex 实体
//!
//! 最终结构：
//! - Brain 直接持有 Cortex 实体 + Memory 记忆系统
//! - Cortex 实体持有 ModelProvider 和 CortexTrait（推理执行）
//! - ModelProvider 只保存配置信息
//! - Memory 持有核心记忆 + 工作记忆

use crate::models::model_provider::ModelProvider;
use anyhow::Result;
use async_trait::async_trait;
use common::enums::ModelCapability;
use dyn_clone::DynClone;

/// 统一的 CortexTrait - 大脑皮层 trait，定义推理接口
#[async_trait]
pub trait CortexTrait: Send + Sync + DynClone {
    /// 返回 Cortex 的能力类型
    fn capability(&self) -> ModelCapability;

    /// 返回模型提供商 ID
    fn model_provider_id(&self) -> &str;

    /// 返回模型名称
    fn model_name(&self) -> &str;

    /// 运行 prompt，获取回答
    async fn prompt(&self, prompt: &str) -> Result<String>;

    /// 生成文本向量 embedding
    async fn embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// 是否支持工具调用
    fn support_tools(&self) -> bool;
}

dyn_clone::clone_trait_object!(CortexTrait);

/// Cortex 实体 - 持有 ModelProvider 和具体的推理实现
///
/// Cortex = 模型配置 + 推理执行
#[derive(Clone)]
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
/// Brain 直接持有 Cortex 实体 + 记忆列表
#[derive(Clone)]
pub struct Brain {
    /// Cortex 实体（包含模型配置 + 推理执行）
    pub cortex: Cortex,
    /// 记忆列表
    pub memories: Vec<crate::models::memory::Memory>,
}

impl Brain {
    /// 创建新 Brain
    pub fn new(cortex: Cortex, memories: Vec<crate::models::memory::Memory>) -> Self {
        Self { cortex, memories }
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
