//! Cortex DAL 层
//!
//! Cortex 业务逻辑层，提供 wake_cortex 测试连通性功能

use anyhow::{Result};
use crate::models::brain::{Cortex, CortexTrait};
use crate::models::model_provider::ModelProvider;
use crate::service::dao::cortex::CortexDao;
use std::sync::{Arc, OnceLock};
use tokio::runtime::Handle;

// ==================== 单例管理 ====================

static CORTEX_DAL: OnceLock<Arc<dyn CortexDal + Send + Sync>> = OnceLock::new();

/// 获取 Cortex DAL 单例
pub fn dal() -> Arc<dyn CortexDal + Send + Sync> {
    CORTEX_DAL.get().cloned().unwrap()
}

/// 初始化 Cortex DAL
pub fn init(cortex_dao: Arc<dyn CortexDao + Send + Sync>) {
    let _ = CORTEX_DAL.set(Arc::new(CortexDalImpl::new(cortex_dao)));
}

// ==================== DAL 接口 ====================

/// Cortex DAL 接口
pub trait CortexDal: Send + Sync {
    /// 创建 Cortex 实体
    fn create_cortex(&self, provider: &ModelProvider) -> Result<Cortex>;

    /// 唤醒 Cortex：创建 Cortex 并执行一次测试调用，验证连通性
    ///
    /// 使用 tokio runtime 阻塞执行异步调用
    fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String>;
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
        self.cortex_dao.create_cortex(provider)
    }

    fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String> {
        let cortex = self.create_cortex(provider)?;

        // 使用 tokio runtime 阻塞执行异步调用
        let result = Handle::current().block_on(async {
            cortex.cortex().prompt(prompt).await
        })?;

        Ok(result)
    }
}
