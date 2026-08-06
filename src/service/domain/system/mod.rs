use crate::consumer::{AopDistributionItem, AopOverview, AopStatsCollector, AopTimeSeriesPoint};
use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::service::dal::backup as backup_dal;
use crate::service::dal::backup::{BackupDal, BackupInfo};
use crate::service::dal::cron_trigger as cron_trigger_dal;
use crate::service::dal::cron_trigger::CronTriggerDal;
use crate::service::dal::log_query as log_query_dal;
use crate::service::dal::log_query::{LogPageResult, LogQuery as LogQueryParam, LogQueryDal};
use crate::service::dao::cron_trigger::CronTriggerQuery;
use common::enums::TriggerType;
use common::error::Result;
use std::sync::{Arc, OnceLock};

mod aop_monitor;
mod aop_stats;
pub mod seed;

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
    /// 通用后台任务注册中心（委托 pkg 全局单例）
    fn background_task_registry(
        &self,
    ) -> &'static crate::pkg::background_task::BackgroundTaskRegistry {
        crate::pkg::background_task::registry()
    }
}

#[async_trait::async_trait]
pub trait CronManager: Send + Sync {
    async fn create_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    async fn get_trigger(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>>;
    async fn list_triggers(
        &self,
        ctx: RequestContext,
        query: CronTriggerQuery,
    ) -> Result<Vec<CronTriggerPo>>;
    async fn update_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    async fn delete_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn pause_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn resume_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn list_due_triggers(
        &self,
        ctx: RequestContext,
        now: i64,
        limit: i32,
    ) -> Result<Vec<CronTriggerPo>>;
    async fn mark_trigger_executed(
        &self,
        ctx: RequestContext,
        id: &str,
        executed_at: i64,
    ) -> Result<()>;
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

    /// 查询日志级别分布（返回 (level, count) 列表）
    async fn level_distribution(
        &self,
        ctx: RequestContext,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<(String, u64)>>;

    /// 查询日志时序（按小时桶，返回 (bucket_start_ms, count) 列表，按时间升序）
    async fn time_series(
        &self,
        ctx: RequestContext,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<(i64, u64)>>;
}

pub trait AopMonitor: Send + Sync {
    fn all_queue_stats(&self) -> Vec<(String, aop::queue::QueueStats)>;
    fn queue_stats(&self, consumer_name: &str) -> Option<aop::queue::QueueStats>;
    fn list_events(
        &self,
        consumer_name: &str,
        filter: aop::queue::EventQueryFilter,
    ) -> Option<Vec<aop::queue::EventSummary>>;
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

    async fn list_triggers(
        &self,
        ctx: RequestContext,
        query: CronTriggerQuery,
    ) -> Result<Vec<CronTriggerPo>> {
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

    async fn list_due_triggers(
        &self,
        ctx: RequestContext,
        now: i64,
        limit: i32,
    ) -> Result<Vec<CronTriggerPo>> {
        self.cron_trigger_dal.list_due(ctx, now, limit).await
    }

    async fn mark_trigger_executed(
        &self,
        ctx: RequestContext,
        id: &str,
        executed_at: i64,
    ) -> Result<()> {
        self.cron_trigger_dal
            .mark_executed(ctx, id, executed_at)
            .await
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
    async fn query_logs(&self, ctx: RequestContext, query: LogQueryParam) -> Result<LogPageResult> {
        self.log_query_dal.query_logs(ctx, query).await
    }

    async fn level_distribution(
        &self,
        ctx: RequestContext,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<(String, u64)>> {
        // 复用 query_logs DAL 方法拉取时间范围内的日志（上限 MAX_SCAN_ENTRIES），
        // 在 Rust 侧做级别聚合。24h 窗口内 10000 条上限对可视化场景足够。
        let query = LogQueryParam {
            keyword: None,
            log_id: None,
            level: None,
            start_time: Some(start_time),
            end_time: Some(end_time),
            page: 1,
            page_size: 10000,
        };
        let result = self.log_query_dal.query_logs(ctx, query).await?;
        let mut dist: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for entry in result.entries {
            *dist.entry(entry.level).or_insert(0) += 1;
        }
        Ok(dist.into_iter().collect())
    }

    async fn time_series(
        &self,
        ctx: RequestContext,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<(i64, u64)>> {
        let query = LogQueryParam {
            keyword: None,
            log_id: None,
            level: None,
            start_time: Some(start_time),
            end_time: Some(end_time),
            page: 1,
            page_size: 10000,
        };
        let result = self.log_query_dal.query_logs(ctx, query).await?;
        let mut buckets: std::collections::HashMap<i64, u64> = std::collections::HashMap::new();
        for entry in result.entries {
            // 解析 ISO8601/RFC3339 时间戳为 unix 毫秒，按小时桶聚合
            if let Some(ts_ms) = chrono::DateTime::parse_from_rfc3339(&entry.timestamp)
                .ok()
                .map(|dt| dt.timestamp_millis())
            {
                let bucket = (ts_ms / 3_600_000) * 3_600_000; // 按小时对齐
                *buckets.entry(bucket).or_insert(0) += 1;
            }
        }
        let mut points: Vec<(i64, u64)> = buckets.into_iter().collect();
        points.sort_by_key(|(ts, _)| *ts);
        Ok(points)
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

// ==================== 系统级默认定时任务 ====================

/// 确保系统级默认定时任务存在（幂等，已有同类型则跳过）
///
/// 设计决策：如果用户已有同 action（agent_rest / project_followup）的触发器
/// 则不重复添加。系统仅提供初始化默认值，用户可自行修改间隔或禁用。
///
/// 通过 payload 字符串包含 `"agent_rest"` / `"project_followup"` 进行去重判断，
/// 与 `CronTriggerConsumer::on_event` 解析 payload.action 的方式保持一致。
pub async fn ensure_system_cron_triggers(ctx: &RequestContext) -> Result<()> {
    let system_domain = domain();
    let cron_manager = system_domain.cron_manager();

    // 获取所有现有 trigger（无过滤条件）
    let existing = cron_manager
        .list_triggers(ctx.clone(), CronTriggerQuery::default())
        .await?;
    let has_agent_rest = existing
        .iter()
        .any(|t| t.payload.contains("\"agent_rest\""));
    let has_project_followup = existing
        .iter()
        .any(|t| t.payload.contains("\"project_followup\""));

    // 1. agent_rest：默认每 4 小时执行一次睡眠沉淀
    if !has_agent_rest {
        let mut trigger = CronTriggerPo::new(
            uuid::Uuid::now_v7().to_string(),
            "系统默认-Agent 睡眠沉淀".into(),
            TriggerType::Interval,
            common::constants::utils::current_timestamp_ms() + 4 * 3600_000,
            Some("system".into()),
        );
        trigger.interval_seconds = Some(4 * 3600);
        trigger.payload = r#"{"action":"agent_rest","extra":{"settle_limit":10}}"#.into();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx.clone(), &trigger).await?;
        sys_info!("已创建系统级定时任务: agent_rest");
    }

    // 2. project_followup：默认每 1 小时执行一次项目进度巡检
    if !has_project_followup {
        let mut trigger = CronTriggerPo::new(
            uuid::Uuid::now_v7().to_string(),
            "系统默认-项目进度巡检".into(),
            TriggerType::Interval,
            common::constants::utils::current_timestamp_ms() + 3600_000,
            Some("system".into()),
        );
        trigger.interval_seconds = Some(3600);
        trigger.payload = r#"{"action":"project_followup","extra":{}}"#.into();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx.clone(), &trigger).await?;
        sys_info!("已创建系统级定时任务: project_followup");
    }

    Ok(())
}
