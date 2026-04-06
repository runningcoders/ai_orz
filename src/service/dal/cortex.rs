//! Cortex DAL 层
//!
//! Cortex 业务逻辑层，提供创建 Cortex 实体和执行 prompt 功能
//! CortexDao 负责创建 CortexTrait 和执行 prompt，DAL 组合 DAO 完成业务功能

use anyhow::{Result};
use crate::models::brain::{Cortex, CortexTrait};
use crate::models::model_provider::ModelProvider;
use crate::service::dao::cortex::{dao as cortex_dao, CortexDao};
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static CORTEX_DAL: OnceLock<Arc<dyn CortexDal + Send + Sync>> = OnceLock::new();

/// 获取 Cortex DAL 单例
pub fn dal() -> Arc<dyn CortexDal + Send + Sync> {
    CORTEX_DAL.get().cloned().unwrap()
}

/// 初始化 Cortex DAL
pub fn init(cortex_dao: Arc<dyn crate::service::dao::cortex::CortexDao + Send + Sync>) {
    let _ = CORTEX_DAL.set(Arc::new(CortexDalImpl::new(cortex_dao)));
}

// ==================== DAL 接口 ====================

/// Cortex DAL 接口
pub trait CortexDal: Send + Sync {
    /// 创建完整 Cortex 实体
    /// 
    /// 调用 CortexDao 创建 CortexTrait，然后组装成完整的 Cortex 实体（包含 ModelProvider + CortexTrait）
    fn create_cortex(&self, provider: &ModelProvider) -> Result<Cortex>;

    /// 唤醒 Cortex：创建 Cortex 并执行 prompt 获取回答
    ///
    /// 使用 tokio runtime 阻塞执行异步调用
    fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String>;

    /// 对已创建的 Cortex 执行 prompt 获取回答
    ///
    /// 使用 tokio runtime 阻塞执行异步调用
    fn prompt_existing_cortex(&self, cortex: &dyn CortexTrait, prompt: &str) -> Result<String>;
}

/// Cortex DAL 实现
pub struct CortexDalImpl {
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
}

impl CortexDalImpl {
    /// 创建 DAL 实例
    pub fn new(cortex_dao: Arc<dyn CortexDao + Send + Sync>) -> Self {
        Self { cortex_dao }
    }
}

impl CortexDal for CortexDalImpl {
    fn create_cortex(&self, provider: &ModelProvider) -> Result<Cortex> {
        let cortex_trait = self.cortex_dao.create_cortex_trait(provider)?;
        Ok(Cortex::new(provider.clone(), cortex_trait))
    }

    fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String> {
        let cortex = self.create_cortex(provider)?;
        self.prompt_existing_cortex(cortex.cortex(), prompt)
    }

    fn prompt_existing_cortex(&self, cortex: &dyn CortexTrait, prompt: &str) -> Result<String> {
        self.cortex_dao.prompt(cortex, prompt)
    }
}
