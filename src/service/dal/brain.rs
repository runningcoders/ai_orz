//! Brain DAL 模块
//!
//! 职责：从已创建的 Cortex 创建完整的 Brain 实体
//! Cortex 由上层创建并传入，BrainDal 只负责组合空的 Memory 和创建 Brain
//! Memory 后续由 AgentDal 装配导入，BrainDal 只负责创建骨架

use crate::error::AppError;
use crate::models::brain::{Brain, Cortex, Memory};
use crate::pkg::RequestContext;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static BRAIN_DAL: OnceLock<Arc<dyn BrainDalTrait>> = OnceLock::new();

/// 获取 Brain DAL 单例
pub fn dal() -> Arc<dyn BrainDalTrait> {
    BRAIN_DAL.get().cloned().unwrap()
}

/// 初始化 Brain DAL
pub fn init() {
    let _ = BRAIN_DAL.set(Arc::new(BrainDal::new()));
}

// ==================== DAL 接口 ====================

/// Brain DAL 接口
pub trait BrainDalTrait: Send + Sync {
    /// 唤醒 Brain：从已创建的 Cortex 创建完整的 Brain
    ///
    /// - Cortex 已经由上层创建好，我们只需要组合空的 Memory
    /// - 记忆后续由 AgentDal 装配导入
    /// - 返回完整的 Brain 骨架实例
    fn wake_brain(
        &self,
        _ctx: RequestContext,
        cortex: Cortex,
        soul: String,
        capabilities: String,
    ) -> Result<Brain, AppError>;
}

// ==================== DAL 实现 ====================

/// Brain DAL 实现
pub struct BrainDal;

impl BrainDal {
    /// 创建 DAL 实例
    pub fn new() -> Self {
        Self
    }
}

impl Default for BrainDal {
    fn default() -> Self {
        Self::new()
    }
}

impl BrainDalTrait for BrainDal {
    fn wake_brain(
        &self,
        _ctx: RequestContext,
        cortex: Cortex,
        soul: String,
        capabilities: String,
    ) -> Result<Brain, AppError> {
        // 1. 创建空的 Memory（从 soul 和 capabilities 初始化 core）
        let memory = Memory::new(soul, capabilities);

        // 2. 创建完整的 Brain 骨架
        let brain = Brain::new(cortex, memory);

        Ok(brain)
    }
}
