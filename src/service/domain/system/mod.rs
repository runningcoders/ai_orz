use common::error::Result;
use crate::consumer::{AopDistributionItem, AopOverview, AopStatsCollector, AopTimeSeriesPoint};
use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::service::dal::backup as backup_dal;
use crate::service::dal::backup::{BackupDal, BackupInfo};
use crate::service::dal::cron_trigger as cron_trigger_dal;
use crate::service::dal::cron_trigger::CronTriggerDal;
use crate::service::dal::log_query as log_query_dal;
use crate::service::dal::log_query::{LogQuery as LogQueryParam, LogPageResult, LogQueryDal};
use crate::service::dao::cron_trigger::CronTriggerQuery;
use std::sync::{Arc, OnceLock};

mod aop_monitor;
mod aop_stats;

static SYSTEM_DOMAIN: OnceLock<Arc<dyn SystemDomain>> = OnceLock::new();

pub fn domain() -> Arc<dyn SystemDomain> {
    SYSTEM_DOMAIN.get().cloned().unwrap()
}

pub fn init() {
    let _ = SYSTEM_DOMAIN.set(new(
        cron_trigger_dal::dal(),
        backup_dal::dal(),
        log_query_dal::dal(),
    ));
}

pub fn new(
    cron_trigger_dal: Arc<dyn CronTriggerDal>,
    backup_dal: Arc<dyn BackupDal + Send + Sync>,
    log_query_dal: Arc<dyn LogQueryDal + Send + Sync>,
) -> Arc<dyn SystemDomain> {
    Arc::new(SystemDomainImpl::new(
        cron_trigger_dal,
        backup_dal,
        log_query_dal,
    ))
}

struct SystemDomainImpl {
    cron_trigger_dal: Arc<dyn CronTriggerDal>,
    backup_dal: Arc<dyn BackupDal + Send + Sync>,
    log_query_dal: Arc<dyn LogQueryDal + Send + Sync>,
}

impl SystemDomainImpl {
    fn new(
        cron_trigger_dal: Arc<dyn CronTriggerDal>,
        backup_dal: Arc<dyn BackupDal + Send + Sync>,
        log_query_dal: Arc<dyn LogQueryDal + Send + Sync>,
    ) -> Self {
        Self {
            cron_trigger_dal,
            backup_dal,
            log_query_dal,
        }
    }
}

impl SystemDomain for SystemDomainImpl {
    fn cron_manager(&self) -> &dyn CronManager {
        self
    }

    fn backup_manager(&self) -> &dyn BackupManager {
        self
    }

    fn log_query(&self) -> &dyn LogQuery {
        self
    }

    fn aop_monitor(&self) -> &dyn AopMonitor {
        self
    }

    fn aop_stats(&self) -> &dyn AopStats {
        self
    }
}

pub trait SystemDomain: Send + Sync {
    fn cron_manager(&self) -> &dyn CronManager;
    fn backup_manager(&self) -> &dyn BackupManager;
    fn log_query(&self) -> &dyn LogQuery;
    fn aop_monitor(&self) -> &dyn AopMonitor;
    fn aop_stats(&self) -> &dyn AopStats;
}

#[async_trait::async_trait]
pub trait CronManager: Send + Sync {
    async fn create_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    async fn get_trigger(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>>;
    async fn list_triggers(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<Vec<CronTriggerPo>>;
    async fn update_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    async fn delete_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn pause_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn resume_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn list_due_triggers(&self, ctx: RequestContext, now: i64, limit: i32) -> Result<Vec<CronTriggerPo>>;
    async fn mark_trigger_executed(&self, ctx: RequestContext, id: &str, executed_at: i64) -> Result<()>;
}

#[async_trait::async_trait]
pub trait BackupManager: Send + Sync {
    async fn create_backup(&self, ctx: RequestContext) -> Result<BackupInfo>;
    async fn list_backups(&self, ctx: RequestContext) -> Result<Vec<BackupInfo>>;
    async fn delete_backup(&self, ctx: RequestContext, version: u64) -> Result<()>;
    async fn generate_restore_script(&self, ctx: RequestContext, version: u64) -> Result<String>;
}

#[async_trait::async_trait]
pub trait LogQuery: Send + Sync {
    async fn query_logs(&self, ctx: RequestContext, query: LogQueryParam) -> Result<LogPageResult>;
}

pub trait AopMonitor: Send + Sync {
    fn all_queue_stats(&self) -> Vec<(String, aop::queue::QueueStats)>;
    fn queue_stats(&self, consumer_name: &str) -> Option<aop::queue::QueueStats>;
    fn list_events(&self, consumer_name: &str, filter: aop::queue::EventQueryFilter) -> Option<Vec<aop::queue::EventSummary>>;
    fn get_event(&self, consumer_name: &str, event_id: &str) -> Option<aop::queue::EventDetail>;
}

#[async_trait::async_trait]
pub trait AopStats: Send + Sync {
    /// 查询概览（全生命周期累计）
    async fn overview(&self, ctx: RequestContext) -> Result<AopOverview>;

    /// 查询时序数据（滑动窗口 60 分钟，按分钟桶）
    async fn time_series(
        &self,
        ctx: RequestContext,
        event_kind: Option<String>,
        consumer_name: Option<String>,
        status: Option<String>,
    ) -> Result<Vec<AopTimeSeriesPoint>>;

    /// 查询分布
    async fn distribution(
        &self,
        ctx: RequestContext,
        group_by: String,
        status_filter: Option<String>,
    ) -> Result<Vec<AopDistributionItem>>;
}

#[async_trait::async_trait]
impl CronManager for SystemDomainImpl {
    async fn create_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        self.cron_trigger_dal.create(ctx, trigger).await
    }

    async fn get_trigger(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>> {
        self.cron_trigger_dal.get_by_id(ctx, id).await
    }

    async fn list_triggers(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<Vec<CronTriggerPo>> {
        self.cron_trigger_dal.list(ctx, query).await
    }

    async fn update_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        self.cron_trigger_dal.update(ctx, trigger).await
    }

    async fn delete_trigger(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.cron_trigger_dal.delete(ctx, id).await
    }

    async fn pause_trigger(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.cron_trigger_dal.pause(ctx, id).await
    }

    async fn resume_trigger(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.cron_trigger_dal.resume(ctx, id).await
    }

    async fn list_due_triggers(&self, ctx: RequestContext, now: i64, limit: i32) -> Result<Vec<CronTriggerPo>> {
        self.cron_trigger_dal.list_due(ctx, now, limit).await
    }

    async fn mark_trigger_executed(&self, ctx: RequestContext, id: &str, executed_at: i64) -> Result<()> {
        self.cron_trigger_dal.mark_executed(ctx, id, executed_at).await
    }
}

#[async_trait::async_trait]
impl BackupManager for SystemDomainImpl {
    async fn create_backup(&self, ctx: RequestContext) -> Result<BackupInfo> {
        self.backup_dal.create_backup(ctx).await
    }

    async fn list_backups(&self, ctx: RequestContext) -> Result<Vec<BackupInfo>> {
        self.backup_dal.list_backups(ctx).await
    }

    async fn delete_backup(&self, ctx: RequestContext, version: u64) -> Result<()> {
        self.backup_dal.delete_backup(ctx, version).await
    }

    async fn generate_restore_script(&self, ctx: RequestContext, version: u64) -> Result<String> {
        self.backup_dal.generate_restore_script(ctx, version).await
    }
}

#[async_trait::async_trait]
impl LogQuery for SystemDomainImpl {
    async fn query_logs(
        &self,
        ctx: RequestContext,
        query: LogQueryParam,
    ) -> Result<LogPageResult> {
        self.log_query_dal.query_logs(ctx, query).await
    }
}

/// 全局 AopStatsCollector 引用（由 lib.rs 在启动时设置）
static AOP_STATS_COLLECTOR: once_cell::sync::OnceCell<AopStatsCollector> =
    once_cell::sync::OnceCell::new();

/// 启动时设置 AopStatsCollector（由 lib.rs 调用）
pub fn set_aop_stats_collector(collector: AopStatsCollector) {
    let _ = AOP_STATS_COLLECTOR.set(collector);
}

/// 获取 AopStatsCollector（内部使用）
pub(crate) fn aop_stats_collector() -> Option<&'static AopStatsCollector> {
    AOP_STATS_COLLECTOR.get()
}