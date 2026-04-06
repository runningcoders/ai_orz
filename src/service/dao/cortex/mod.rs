//! Cortex DAO - 大脑皮层工厂
//!
//! 根据 Model Provider 创建 Cortex 实体，提供统一推理接口

use anyhow::{Result, anyhow};
use std::sync::{Arc, OnceLock};
use crate::models::{self, brain::*, model_provider::ModelProviderPo};

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
#[async_trait::async_trait]
pub trait CortexDao: Send + Sync {
    /// 根据 Model Provider 创建 Cortex
    fn create_cortex(&self, provider: &ModelProviderPo) -> Result<Box<dyn Cortex + Send + Sync>>;
}

mod rig;

pub use rig::{RigCortexDao};

#[cfg(test)]
mod rig_test;
