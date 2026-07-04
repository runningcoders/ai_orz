//! Model Provider DAL 模块

use common::error::Result;
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::dao::model_provider;
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderQuery};
use common::enums::ModelProviderStatus;
use std::sync::{Arc, OnceLock};

use crate::enrich_ctx;
// ==================== 单例管理 ====================

static MODEL_PROVIDER_DAL: OnceLock<Arc<dyn ModelProviderDal>> = OnceLock::new();

/// 获取 Model Provider DAL 单例
pub fn dal() -> Arc<dyn ModelProviderDal> {
    MODEL_PROVIDER_DAL.get().cloned().unwrap()
}

/// 初始化 Model Provider DAL
pub fn init() {
    let _ = MODEL_PROVIDER_DAL.set(new(model_provider::dao()));
}

/// 创建 Model Provider DAL（返回 trait 对象）
pub fn new(
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
) -> Arc<dyn ModelProviderDal> {
    Arc::new(ModelProviderDalImpl { model_provider_dao })
}

// ==================== DAL 实现 ====================

/// Model Provider DAL 接口
#[async_trait::async_trait]
pub trait ModelProviderDal: Send + Sync {
    /// 创建 Model Provider
    async fn create(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()>;

    /// 根据 ID 查询 Model Provider
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProvider>>;

    /// 查询所有 Model Provider
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>>;

    /// 通用综合查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: ModelProviderQuery,
    ) -> Result<Vec<ModelProvider>>;

    /// 更新 Model Provider
    async fn update(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()>;

    /// 删除 Model Provider
    async fn delete(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()>;
}

/// Model Provider DAL 实现
struct ModelProviderDalImpl {
    model_provider_dao: Arc<dyn ModelProviderDao>,
}

impl ModelProviderDalImpl {
    /// 创建 DAL 实例
    fn new(model_provider_dao: Arc<dyn ModelProviderDao>) -> Self {
        Self { model_provider_dao }
    }
}

#[async_trait::async_trait]
impl ModelProviderDal for ModelProviderDalImpl {
    async fn create(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dao.insert(ctx, &provider.po).await
    }

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProvider>> {
        let opt = self.model_provider_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(ModelProvider::from_po))
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>> {
        self.query(
            ctx,
            ModelProviderQuery {
                exclude_status: Some(ModelProviderStatus::Deleted),
                ..Default::default()
            },
        )
        .await
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: ModelProviderQuery,
    ) -> Result<Vec<ModelProvider>> {
        let providers = self.model_provider_dao.query(ctx, query).await?;
        Ok(providers.into_iter().map(ModelProvider::from_po).collect())
    }

    async fn update(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dao.update(ctx, &provider.po).await
    }

    async fn delete(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dao.delete(ctx, &provider.po).await
    }
}
