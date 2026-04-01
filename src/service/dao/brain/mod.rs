//! Brain DAO - 大脑工厂
//!
//! 根据 Model Provider 创建 Brain 实体，提供统一推理接口

use anyhow::Result;
use std::sync::{Arc, OnceLock};
use crate::models::{self, brain::*, model_provider::ModelProviderPo};

// ==================== 单例 ====================

static BRAIN_DAO: OnceLock<Arc<dyn BrainDao + Send + Sync>> = OnceLock::new();

/// 获取 BrainDAO 单例
pub fn dao() -> Arc<dyn BrainDao + Send + Sync> {
    BRAIN_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = BRAIN_DAO.set(Arc::new(rig::RigBrainDao::new()));
}

/// Brain DAO 工厂 trait
#[async_trait::async_trait]
pub trait BrainDao: Send + Sync {
    /// 根据 Model Provider 创建 Brain
    fn create_brain(&self, provider: &ModelProviderPo) -> Result<Brain>;
    
    /// 统一调用：运行 prompt，获取回答
    async fn prompt(&self, brain: &Brain, prompt: &str) -> Result<String>;
}

mod rig;
pub use rig::{RigBrainDao};

#[cfg(test)]
mod rig_test;
