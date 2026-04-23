//! Finance Domain 模块
//!
//! 财务领域模块，管理：
//! - ModelProvider - 大语言模型提供商

pub mod model_provider;

#[cfg(test)]
mod model_provider_test;

use std::sync::{Arc, OnceLock};
use crate::pkg::RequestContext;
use crate::models::model_provider::ModelProvider;
use crate::service::dal::model_provider as model_provider_dal;

// ==================== trait 定义 ====================

/// Finance Domain 总 trait
///
/// 聚合财务领域模块所有子功能 trait
pub trait FinanceDomain: Send + Sync {
    /// Model Provider 管理能力
    fn model_provider_manage(&self) -> &dyn ModelProviderManage;
}

/// Model Provider 管理 trait
///
/// 定义 Model Provider 相关的业务接口
#[async_trait::async_trait]
pub trait ModelProviderManage: Send + Sync {
    /// 创建 Model Provider
    async fn create_model_provider(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), crate::error::AppError>;

    /// 获取 Model Provider
    async fn get_model_provider(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>, crate::error::AppError>;

    /// 列出所有 Model Provider
    async fn list_model_providers(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>, crate::error::AppError>;

    /// 更新 Model Provider
    async fn update_model_provider(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), crate::error::AppError>;

    /// 删除 Model Provider
    async fn delete_model_provider(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), crate::error::AppError>;

    /// 唤醒 Cortex：创建临时 Cortex 并执行调用
    ///
    /// 上层已经查询好 Model Provider，直接传递进来
    /// - provider: 模型提供商
    /// - prompt: 调用提示词
    /// - 返回: 模型输出结果
    async fn wake_cortex(&self, ctx: RequestContext, provider: &ModelProvider, prompt: &str) -> Result<String, crate::error::AppError>;
}

// ==================== 单例管理 ====================

static FINANCE_DOMAIN: OnceLock<Arc<dyn FinanceDomain>> = OnceLock::new();

/// 获取 Finance Domain 单例
pub fn domain() -> Arc<dyn FinanceDomain> {
    FINANCE_DOMAIN.get().cloned().unwrap()
}

/// 初始化 Finance Domain
pub fn init() {
    let finance_domain = FinanceDomainImpl::new(
        model_provider_dal::dal()
    );
    let _ = FINANCE_DOMAIN.set(Arc::new(finance_domain));
}

// ==================== 实现 ====================

/// Finance Domain 实现
///
/// 聚合所有财务领域子功能实现
struct FinanceDomainImpl {
    model_provider_dal: Arc<dyn model_provider_dal::ModelProviderDal>,
}

impl FinanceDomainImpl {
    /// 创建 Domain 实例
    fn new(model_provider_dal: Arc<dyn model_provider_dal::ModelProviderDal>) -> Self {
        Self { model_provider_dal }
    }
}

impl FinanceDomain for FinanceDomainImpl {
    fn model_provider_manage(&self) -> &dyn ModelProviderManage {
        self
    }
}
