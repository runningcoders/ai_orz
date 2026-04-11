//! Model Provider DAL 模块

use crate::error::AppError;
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::dao::model_provider::ModelProviderDaoTrait;
use std::sync::{Arc, OnceLock};
use crate::service::dao::model_provider;
// ==================== 单例管理 ====================

static MODEL_PROVIDER_DAL: OnceLock<Arc<dyn ModelProviderDalTrait>> = OnceLock::new();

/// 获取 Model Provider DAL 单例
pub fn dal() -> Arc<dyn ModelProviderDalTrait> {
    MODEL_PROVIDER_DAL.get().cloned().unwrap()
}

/// 初始化 Model Provider DAL
pub fn init() {
    // 先初始化 DAO，再初始化 DAL
    crate::service::dao::model_provider::init();
    let _ = MODEL_PROVIDER_DAL.set(Arc::new(ModelProviderDal::new(
        model_provider::dao(),
    )));
}

// ==================== DAL 实现 ====================

/// Model Provider DAL 接口
#[async_trait::async_trait]
pub trait ModelProviderDalTrait: Send + Sync {
    /// 创建 Model Provider
    async fn create(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError>;

    /// 根据 ID 查询 Model Provider
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>, AppError>;

    /// 查询所有 Model Provider
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>, AppError>;

    /// 更新 Model Provider
    async fn update(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError>;

    /// 删除 Model Provider
    async fn delete(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError>;
}

/// Model Provider DAL 实现
pub struct ModelProviderDal {
    model_provider_dao: Arc<dyn ModelProviderDaoTrait>,
}

impl ModelProviderDal {
    /// 创建 DAL 实例
    pub fn new(
        model_provider_dao: Arc<dyn ModelProviderDaoTrait>,
    ) -> Self {
        Self { model_provider_dao }
    }
}

#[async_trait::async_trait]
impl ModelProviderDalTrait for ModelProviderDal {
    async fn create(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError> {
        self.model_provider_dao.insert(ctx, &provider.po).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>, AppError> {
        let opt = self.model_provider_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(ModelProvider::from_po))
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>, AppError> {
        let providers = self.model_provider_dao.find_all(ctx).await?;
        Ok(providers.into_iter().map(ModelProvider::from_po).collect())
    }

    async fn update(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError> {
        self.model_provider_dao.update(ctx, &provider.po).await
    }

    async fn delete(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError> {
        self.model_provider_dao.delete(ctx, &provider.po).await
    }
}
