//! Model Provider DAL 模块

use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::pkg::stats::{DECISION_CALL_KIND, ModelCallEvent, StatFilter};
use crate::service::dao::model_provider;
use crate::service::dao::model_provider::{
    ModelProviderDao, ModelProviderQuery, ModelProviderStatsDao, ModelProviderStatsQuery,
};
use common::api::PagedResult;
use common::enums::{ModelCapability, ModelProviderStatus};
use common::error::Result;
use common::models::{ModelCallStats, StatsFetchOptions, StatsInterval, TimeSeriesPoint};
use serde_json::Value as JsonValue;
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
    Arc::new(ModelProviderDalImpl {
        model_provider_dao,
        model_provider_stats_dao,
    })
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
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>>;

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
    ) -> Result<PagedResult<ModelProvider>>;

    /// 更新 Model Provider
    async fn update(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()>;

    /// 删除 Model Provider
    async fn delete(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()>;

    /// 获取当前启用的 Embedding Provider（用于唯一性校验）
    async fn find_enabled_embedding_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProvider>>;

    /// 获取当前启用的小脑 Provider（Decision 单启用守卫用）
    async fn find_enabled_decision_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProvider>>;

    // ==================== 统计查询 ====================

    /// 获取 ModelProvider 统计数据（按 options 控制返回哪些维度）
    ///
    /// `call_kind` 决定统计口径：对话 Provider 用 `chat`，向量 Provider 用
    /// `embedding`，避免两类调用在详情页混算。
    async fn get_stats(
        &self,
        ctx: RequestContext,
        model_provider_id: &str,
        call_kind: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats>;

    /// 查询组织级分钟级模型调用时序（工作台顶栏 Token QPS 曲线用）
    ///
    /// 组织隔离由 `ctx.organization_id` 注入 filter 实现；系统上下文（无组织）时统计全量。
    /// `minutes` 会被 clamp 到 `[1, 1440]`（最长 24 小时）。
    async fn model_call_time_series(
        &self,
        ctx: RequestContext,
        minutes: u32,
    ) -> Result<Vec<TimeSeriesPoint>>;

    /// 按触发用户查询模型调用统计（用户详情页看板用）
    ///
    /// 口径为「打点命中」：仅统计埋点 `user_id` 与目标一致的调用，
    /// Agent 自主思考 / 定时任务等无用户上下文的消耗不计入。
    /// 组织隔离由 `ctx.organization_id` 注入 filter 实现。
    async fn get_model_call_stats_for_user(
        &self,
        ctx: RequestContext,
        user_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats>;
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

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>> {
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

                let call_kind = call_kind_for_capability(provider.po.capability);
                match self
                    .get_stats(ctx.clone(), id, call_kind, stats_options)
                    .await
                {
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
        let page = self
            .query(
                ctx,
                ModelProviderQuery {
                    exclude_status: Some(ModelProviderStatus::Deleted),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: ModelProviderQuery,
    ) -> Result<PagedResult<ModelProvider>> {
        let page = self.model_provider_dao.query(ctx, query).await?;
        Ok(page.map(ModelProvider::from_po))
    }

    async fn update(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dao.update(ctx, &provider.po).await
    }

    async fn delete(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dao.delete(ctx, &provider.po).await
    }

    async fn find_enabled_embedding_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProvider>> {
        match self
            .model_provider_dao
            .find_enabled_embedding_provider(ctx)
            .await?
        {
            Some(po) => Ok(Some(ModelProvider { po, stats: None })),
            None => Ok(None),
        }
    }

    async fn find_enabled_decision_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProvider>> {
        match self
            .model_provider_dao
            .find_enabled_decision_provider(ctx)
            .await?
        {
            Some(po) => Ok(Some(ModelProvider { po, stats: None })),
            None => Ok(None),
        }
    }

    // ==================== 统计查询 ====================

    async fn get_stats(
        &self,
        ctx: RequestContext,
        model_provider_id: &str,
        call_kind: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats> {
        let query = ModelProviderStatsQuery {
            model_provider_id: Some(model_provider_id.to_string()),
            call_kind: Some(call_kind.to_string()),
            time_range: options.time_range,
            interval: options.interval,
            ..Default::default()
        };
        self.model_provider_stats_dao
            .get_stats(ctx, query, options)
            .await
    }

    async fn model_call_time_series(
        &self,
        ctx: RequestContext,
        minutes: u32,
    ) -> Result<Vec<TimeSeriesPoint>> {
        const MAX_MINUTES: u32 = 1440;
        let minutes = minutes.clamp(1, MAX_MINUTES);
        let now = chrono::Utc::now().timestamp_millis();
        let start = now - i64::from(minutes) * 60_000;

        let mut query = ModelProviderStatsQuery {
            time_range: Some((start, now)),
            interval: Some(StatsInterval::Minutely),
            ..Default::default()
        };
        if let Some(org_id) = ctx.organization_id.clone() {
            query.filters.push(StatFilter::Equals {
                key: "organization_id".to_string(),
                value: JsonValue::String(org_id),
            });
        }

        self.model_provider_stats_dao
            .query_model_call_time_series(ctx, query)
            .await
    }

    async fn get_model_call_stats_for_user(
        &self,
        ctx: RequestContext,
        user_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats> {
        let mut query = ModelProviderStatsQuery {
            user_id: Some(user_id.to_string()),
            time_range: options.time_range,
            interval: options.interval,
            ..Default::default()
        };
        if let Some(org_id) = ctx.organization_id.clone() {
            query.filters.push(StatFilter::Equals {
                key: "organization_id".to_string(),
                value: JsonValue::String(org_id),
            });
        }

        self.model_provider_stats_dao
            .get_stats(ctx, query, options)
            .await
    }
}

/// 模型调用统计的 `call_kind` 取值：由 provider 的 capability 唯一确定
///
/// ## 为什么按 capability 派生
///
/// 查询侧**拿不到调用点**——模型详情页只知道「这个 provider 是谁」，不知道它
/// 被哪些链路用过。所以「详情页该查哪个桶」只能由 provider 自身属性推断。
///
/// ## 为什么必须穷举 match（不许 `_` 兜底）
///
/// 此前这里是 `Embedding => "embedding", _ => "chat"`，`Decision` 就静默落进
/// 了 chat 桶：小脑调用要么查不到，要么把对话口径虚高。新增 capability 时
/// 本函数必须编译不过，逼作者显式回答「新种类记进哪个桶」，而不是被 `_`
/// 悄悄并进 chat。录制侧与本函数共用同一份字符串契约（见
/// `dal::brain::decision_model_call_event`）。
fn call_kind_for_capability(capability: ModelCapability) -> &'static str {
    match capability {
        ModelCapability::Agent => "chat",
        ModelCapability::Embedding => "embedding",
        ModelCapability::Decision => DECISION_CALL_KIND,
    }
}

#[cfg(test)]
mod call_kind_tests {
    use super::*;

    /// 三种 capability 各自映射到自己的桶，一个都不能落进 chat
    #[test]
    fn call_kind_maps_each_capability_to_its_own_bucket() {
        assert_eq!(call_kind_for_capability(ModelCapability::Agent), "chat");
        assert_eq!(
            call_kind_for_capability(ModelCapability::Embedding),
            "embedding"
        );
        assert_eq!(
            call_kind_for_capability(ModelCapability::Decision),
            "decision"
        );
    }

    /// 两两不等：专防「将来有人把 Decision 也映射成 chat」——
    /// 那种改动在逐条字面断言下看起来「对」，实际上把小脑调用灌进对话口径。
    #[test]
    fn call_kind_values_are_pairwise_distinct() {
        let agent = call_kind_for_capability(ModelCapability::Agent);
        let embedding = call_kind_for_capability(ModelCapability::Embedding);
        let decision = call_kind_for_capability(ModelCapability::Decision);
        assert_ne!(agent, embedding);
        assert_ne!(agent, decision);
        assert_ne!(embedding, decision);
    }
}
