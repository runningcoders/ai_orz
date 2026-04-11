//! Cortex DAO - 大脑皮层工厂
//!
//! 根据 Model Provider 创建 CortexTrait 实例，提供统一推理接口
//! 包含 create_cortex_trait 和 prompt（执行 prompt 获取回答）

use anyhow::{Result};
use crate::models::{brain::*, model_provider::ModelProvider};
use crate::pkg::RequestContext;

/// Cortex DAO 工厂 trait
///
/// CortexDao 负责创建 CortexTrait 和 prompt，所有方法都传递 ctx
#[async_trait::async_trait]
pub trait CortexDao: Send + Sync {
    /// 根据 Model Provider 创建 CortexTrait 实例
    fn create_cortex_trait(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<Box<dyn CortexTrait + Send + Sync>>;

    /// 执行 prompt：使用已创建的 CortexTrait 推理获取回答
    async fn prompt(&self, ctx: RequestContext, cortex: &dyn CortexTrait, prompt: &str) -> Result<String>;
}

mod rig;

pub use self::rig::{dao, init};

#[cfg(test)]
mod rig_test;
