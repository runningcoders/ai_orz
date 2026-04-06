//! Model Provider DAL 模块

use crate::error::AppError;
use crate::models::brain::Cortex;
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use crate::pkg::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDaoTrait;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static MODEL_PROVIDER_DAL: OnceLock<Arc<dyn ModelProviderDalTrait>> = OnceLock::new();

/// 获取 Model Provider DAL 单例
pub fn dal() -> Arc<dyn ModelProviderDalTrait> {
    MODEL_PROVIDER_DAL.get().cloned().unwrap()
}

/// 初始化 Model Provider DAL
pub fn init(
    model_provider_dao: Arc<dyn ModelProviderDaoTrait>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
) {
    let _ = MODEL_PROVIDER_DAL.set(Arc::new(ModelProviderDal::new(model_provider_dao, cortex_dao)));
}

// ==================== DAL 实现 ====================

/// Model Provider DAL 接口
pub trait ModelProviderDalTrait: Send + Sync {
    /// 创建 Model Provider
    fn create(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError>;

    /// 根据 ID 查询 Model Provider
    fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>, AppError>;

    /// 查询所有 Model Provider
    fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>, AppError>;

    /// 更新 Model Provider
    fn update(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError>;

    /// 删除 Model Provider
    fn delete(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError>;

    /// 唤醒 Cortex：创建 Cortex 并执行一次测试调用，验证连通性
    ///
    /// 根据 Model Provider 配置创建 Cortex，然后执行 prompt 返回结果
    fn wake_cortex(&self, provider: &ModelProviderPo, prompt: &str) -> Result<String, AppError>;
}

/// Model Provider DAL 实现
pub struct ModelProviderDal {
    model_provider_dao: Arc<dyn ModelProviderDaoTrait>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
}

impl ModelProviderDal {
    /// 创建 DAL 实例
    pub fn new(
        model_provider_dao: Arc<dyn ModelProviderDaoTrait>,
        cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    ) -> Self {
        Self { model_provider_dao, cortex_dao }
    }
}

impl ModelProviderDalTrait for ModelProviderDal {
    fn create(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError> {
        self.model_provider_dao.insert(ctx, &provider.po)
    }

    fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>, AppError> {
        self.model_provider_dao
            .find_by_id(ctx, id)
            .map(|opt| opt.map(ModelProvider::from_po))
    }

    fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>, AppError> {
        self.model_provider_dao
            .find_all(ctx)
            .map(|providers: Vec<_>| providers.into_iter().map(ModelProvider::from_po).collect())
    }

    fn update(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError> {
        self.model_provider_dao.update(ctx, &provider.po)
    }

    fn delete(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError> {
        self.model_provider_dao.delete(ctx, &provider.po)
    }

    fn wake_cortex(&self, provider: &ModelProviderPo, prompt: &str) -> Result<String, AppError> {
        let cortex = self.cortex_dao.create_cortex(provider)?;

        // 使用 tokio runtime 阻塞执行异步调用
        let result = tokio::runtime::Handle::current().block_on(async {
            cortex.prompt(prompt).await
        })?;

        Ok(result)
    }
}
