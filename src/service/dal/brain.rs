//! Brain DAL 模块
//!
//! 职责：从已创建的 Cortex 和已创建的 Memory 创建完整的 Brain 实体
//! Cortex 和 Memory 都由上层创建并传入，BrainDal 只负责组合
//! 完全解耦，遵循单一职责和同级不调用规范

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
    /// 唤醒 Brain：从已创建的 Cortex 和已创建的 Memory 创建完整的 Brain
    ///
    /// - Cortex 已经由上层创建好
    /// - Memory 已经由上层创建好
    /// - BrainDal 只负责组合成完整的 Brain 实例
    fn wake_brain(
        &self,
        _ctx: RequestContext,
        cortex: Cortex,
        memory: Memory,
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
        memory: Memory,
    ) -> Result<Brain, AppError> {
        // 只负责组合，Cortex 和 Memory 都由上层创建
        let brain = Brain::new(cortex, memory);
        Ok(brain)
    }
}
