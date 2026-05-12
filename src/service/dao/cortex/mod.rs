//! Cortex DAO - 大脑皮层工厂
//!
//! 根据 Model Provider 创建 CortexTrait 实例，提供统一推理接口
//! 包含 create_cortex_trait 和 prompt（执行 prompt 获取回答）

use anyhow::Result;
use crate::models::brain::*;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::request_context::RequestContext;
use ::rig::tool::ToolDyn;

/// Cortex DAO 工厂 trait
///
/// CortexDao 负责创建 CortexTrait 和执行推理/向量化，所有方法都传递 ctx
#[async_trait::async_trait]
pub trait CortexDao: Send + Sync {
    /// ✅ 根据 Model Provider 创建 CortexTrait 实例，绑定已包装的 Rig 工具列表
    fn create_cortex_trait(&self, ctx: RequestContext, provider: &ModelProviderPo, rig_tools: Vec<Box<dyn ToolDyn>>) -> Result<Box<dyn CortexTrait + Send + Sync>>;

    /// ✅ 执行 prompt：使用已创建的 CortexTrait 推理获取回答
    async fn prompt(&self, ctx: RequestContext, cortex: &dyn CortexTrait, prompt: &str) -> Result<String>;

    /// ✅ 文本转向量
    async fn embed_text(&self, ctx: RequestContext, cortex: &dyn CortexTrait, text: &str) -> Result<Vec<f32>>;
}

mod rig;

pub use self::rig::{dao, init, RigCortexDao};

#[cfg(test)]
mod rig_test;
