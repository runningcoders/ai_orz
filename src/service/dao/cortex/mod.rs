//! Cortex DAO - 大脑皮层工厂
//!
//! 根据 Model Provider 创建 CortexTrait 实例，提供统一推理接口
//! 包含 create_cortex_trait 和 prompt（执行 prompt 获取回答）

use anyhow::{Result};
use crate::models::{self, brain::*, model_provider::ModelProvider};
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static CORTEX_DAO: OnceLock<Arc<dyn CortexDao + Send + Sync>> = OnceLock::new();

/// 获取 CortexDAO 单例
pub fn dao() -> Arc<dyn CortexDao + Send + Sync> {
    CORTEX_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = CORTEX_DAO.set(Arc::new(rig::RigCortexDao::new()));
}

/// Cortex DAO 工厂 trait
///
/// CortexDao 负责创建 CortexTrait 和执行 prompt，所有调用收敛到 DAO
#[async_trait::async_trait]
pub trait CortexDao: Send + Sync {
    /// 根据 Model Provider 创建 CortexTrait 实例
    fn create_cortex_trait(&self, provider: &ModelProvider) -> Result<Box<dyn CortexTrait + Send + Sync>>;

    /// 执行 prompt：使用已创建的 CortexTrait 推理获取回答
    ///
    /// 使用 tokio runtime 阻塞执行异步调用
    fn prompt(&self, cortex: &dyn CortexTrait, prompt: &str) -> Result<String>;
}

mod rig;

pub use rig::{RigCortexDao};

#[cfg(test)]
mod rig_test;
