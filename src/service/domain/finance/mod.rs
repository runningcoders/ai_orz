//! Finance Domain 模块
//!
//! 财务领域模块，管理：
//! - ModelProvider - 大语言模型提供商

pub mod model_provider;

use crate::error::AppError;
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::dal::model_provider::ModelProviderDalTrait;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static FINANCE_DOMAIN: OnceLock<Arc<dyn FinanceDomain>> = OnceLock::new();

/// 获取 Finance Domain 单例
pub fn domain() -> Arc<dyn FinanceDomain> {
    FINANCE_DOMAIN.get().cloned().unwrap()
}

/// 初始化 Finance Domain
pub fn init(model_provider_dal: Arc<dyn ModelProviderDalTrait>) {
    let finance_domain = FinanceDomainImpl::new(model_provider_dal);
    let _ = FINANCE_DOMAIN.set(Arc::new(finance_domain));
}

// ==================== 实现 ====================

/// Finance Domain 实现
///
/// 聚合所有财务领域子功能实现
pub struct FinanceDomainImpl {
    model_provider_dal: Arc<dyn ModelProviderDalTrait>,
}

impl FinanceDomainImpl {
    /// 创建 Domain 实例
    pub fn new(model_provider_dal: Arc<dyn ModelProviderDalTrait>) -> Self {
        Self { model_provider_dal }
    }
}

// ==================== traits 定义 ====================

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
pub trait ModelProviderManage: Send + Sync {
    /// 创建 Model Provider
    fn create_model_provider(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError>;

    /// 获取 Model Provider
    fn get_model_provider(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>, AppError>;

    /// 列出所有 Model Provider
    fn list_model_providers(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>, AppError>;

    /// 更新 Model Provider
    fn update_model_provider(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError>;

    /// 删除 Model Provider
    fn delete_model_provider(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError>;

    /// 唤醒 Cortex：创建临时 Cortex 并执行调用
    ///
    /// 用于测试连通性或直接调用模型
    /// - provider_id: 模型 ID
    /// - prompt: 调用提示词
    /// - 返回: 模型输出结果
    fn wake_cortex(&self, ctx: RequestContext, id: &str, prompt: &str) -> Result<String, AppError>;
}

impl FinanceDomain for FinanceDomainImpl {
    fn model_provider_manage(&self) -> &dyn ModelProviderManage {
        self
    }
}
