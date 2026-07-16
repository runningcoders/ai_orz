//! Model Provider DAL 模块

use common::error::Result;
use common::models::{ModelCallStats, StatsFetchOptions};
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::pkg::stats::ModelCallEvent;
use crate::service::dao::model_provider;
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderQuery, ModelProviderStatsDao, ModelProviderStatsQuery};
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
    model_provider::stats_init();
    let _ = MODEL_PROVIDER_DAL.set(new(model_provider::dao(), model_provider::stats_dao()));
}

/// 创建 Model Provider DAL（返回 trait 对象）
pub fn new(
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
) -> Arc<dyn ModelProviderDal> {
    Arc::new(ModelProviderDalImpl { model_provider_dao, model_provider_stats_dao })
}

// ==================== DAL 接口 ====================

/// Model Provider 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct ModelProviderFetchOptions {
    /// 是否加载模型调用统计（ModelCallStats）
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<common::models::StatsInterval>,
}

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

    /// 根据 ID 查询 Model Provider（带附带信息选项）
    async fn get_model_provider(
        &self,
        ctx: RequestContext,
        id: &str,
        options: ModelProviderFetchOptions,
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

    /// 获取当前启用的 Embedding Provider（用于唯一性校验）
    async fn find_enabled_embedding_provider(&self, ctx: RequestContext) -> Result<Option<ModelProvider>>;

    // ==================== 统计查询 ====================

    /// 获取 ModelProvider 统计数据（按 options 控制返回哪些维度）
    async fn get_stats(&self, ctx: RequestContext, model_provider_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats>;
}

/// Model Provider DAL 实现
struct ModelProviderDalImpl {
    model_provider_dao: Arc<dyn ModelProviderDao>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
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

    async fn get_model_provider(
        &self,
        ctx: RequestContext,
        id: &str,
        options: ModelProviderFetchOptions,
    ) -> Result<Option<ModelProvider>> {
        // Step 1: 获取基础 ModelProvider 实体
        let mut provider = self.find_by_id(ctx.clone(), id).await?;

        if let Some(ref mut provider) = provider {
            // Step 2: 按 options 注入 model_call_stats
            if options.with_model_call_stats.unwrap_or(false) {
                let stats_options = StatsFetchOptions {
                    with_call_summary: true,
                    with_token_summary: true,
                    with_time_series: true,
                    time_range: options.stats_time_range,
                    interval: options.stats_interval,
                };

                match self.get_stats(ctx.clone(), id, stats_options).await {
                    Ok(stats) => {
                        provider.stats = Some(stats);
                    }
                    Err(e) => {
                        log_warn!(&ctx, "get_model_provider", model_provider_id = %id, error = ?e, "模型调用统计注入失败，已降级");
                    }
                }
            }
        }

        Ok(provider)
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

    async fn find_enabled_embedding_provider(&self, ctx: RequestContext) -> Result<Option<ModelProvider>> {
        match self.model_provider_dao.find_enabled_embedding_provider(ctx).await? {
            Some(po) => Ok(Some(ModelProvider { po, stats: None })),
            None => Ok(None),
        }
    }

    // ==================== 统计查询 ====================

    async fn get_stats(&self, ctx: RequestContext, model_provider_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats> {
        let query = ModelProviderStatsQuery {
            model_provider_id: Some(model_provider_id.to_string()),
            time_range: options.time_range,
            interval: options.interval,
            ..Default::default()
        };
        self.model_provider_stats_dao.get_stats(ctx, query, options).await
    }
}
