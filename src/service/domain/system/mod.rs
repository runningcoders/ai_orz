use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::service::dal::backup as backup_dal;
use crate::service::dal::backup::{BackupDal, BackupInfo};
use crate::service::dal::cron_trigger as cron_trigger_dal;
use crate::service::dal::cron_trigger::CronTriggerDal;
use crate::service::dal::log_query as log_query_dal;
use crate::service::dal::log_query::{LogQuery as LogQueryParam, LogQueryDal, QueryLogsResponse};
use crate::service::dao::cron_trigger::CronTriggerQuery;
use common::enums::{TriggerBehavior, TriggerType};
use common::error::{Result, bail_err, err};
use std::sync::{Arc, OnceLock};

mod aop_monitor;
mod process;
pub mod scene_projection;
pub(crate) mod schedule_mutation;
pub(crate) mod schedule_query;
pub mod seed;

pub use process::ProcessStatusDetail;
pub use schedule_mutation::ScheduleUpdate;
pub use schedule_query::ScheduleQuery;

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

/// 第二阶段：异步注入本 Domain 的基础数据（幂等）。
///
/// 与同步的 `init()`（只设 OnceLock 单例）分离，原因：
/// - 基础数据依赖 DB IO，必须 async；
/// - 测试里大量用 `Once::call_once(|| ...)` 同步闭包调 `init()`，不能在里面 .await；
/// - 基础数据失败不影响模块本身已注册可用，记录 warn 即可。
pub async fn init_base_data() {
    let ctx = RequestContext::new_system();
    match ensure_system_cron_triggers(&ctx).await {
        Ok(()) => sys_info!("system domain 基础数据初始化完成（cron triggers）"),
        Err(e) => sys_warn!("system domain 基础数据初始化失败（cron triggers）: {}", e),
    }
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

/// 测试专用：用无状态 DAL 实例构造 SystemDomain（不依赖全局单例初始化）
#[cfg(test)]
pub(crate) fn new_for_test() -> Arc<dyn SystemDomain> {
    new(
        cron_trigger_dal::new(crate::service::dao::cron_trigger::new(), None),
        backup_dal::new(),
        log_query_dal::new(),
    )
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

    fn process_manager(&self) -> &dyn ProcessManager {
        self
    }
}

pub trait SystemDomain: Send + Sync {
    fn cron_manager(&self) -> &dyn CronManager;
    fn backup_manager(&self) -> &dyn BackupManager;
    fn log_query(&self) -> &dyn LogQuery;
    fn aop_monitor(&self) -> &dyn AopMonitor;
    fn process_manager(&self) -> &dyn ProcessManager;
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
    /// 查询触发器。过滤条件可直接用 `CronTriggerQuery` 表达，故不额外造领域参数。
    async fn list_triggers(
        &self,
        ctx: RequestContext,
        query: CronTriggerQuery,
    ) -> Result<Vec<CronTriggerPo>>;
    /// 按同一条件统计触发器总数（不含分页）
    async fn count_triggers(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<usize>;

    /// 查询某触发器（日程维度）的执行明细分页
    async fn query_trigger_exec(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::cron_trigger::TriggerExecQuery,
    ) -> Result<crate::service::dao::cron_trigger::TriggerExecQueryResult>;

    /// 查询执行概览与时序分布（独立于明细分页）
    async fn query_trigger_exec_stats(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::cron_trigger::TriggerExecStatsQuery,
    ) -> Result<crate::service::dao::cron_trigger::TriggerExecStatsResult>;

    /// 查询某用户在指定时间范围的日程实例。
    ///
    /// 身份从 ctx 固化，不做参数化 owner；日期词解析、关键词过滤、规则展开、
    /// 排序截断全部在 domain 完成，handler 与 consumer 共用同一份语义。
    async fn query_schedules(
        &self,
        ctx: RequestContext,
        query: ScheduleQuery,
    ) -> Result<common::api::QueryScheduleResponse>;
    async fn update_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    /// Agent 取消自己的日程（软删、幂等）。返回本次是否真的取消。
    async fn cancel_schedule(&self, ctx: RequestContext, trigger_id: &str) -> Result<bool>;
    /// Agent 更新自己的日程（只开放安全字段，身份从 ctx 固化）。
    async fn update_schedule(
        &self,
        ctx: RequestContext,
        update: ScheduleUpdate,
    ) -> Result<CronTriggerPo>;
    async fn delete_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn pause_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn resume_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn mark_trigger_executed(
        &self,
        ctx: RequestContext,
        id: &str,
        executed_at: i64,
    ) -> Result<()>;

    /// 原子领取到期触发器（短租约，防多实例重复触发）
    async fn claim_due_triggers(
        &self,
        ctx: RequestContext,
        now: i64,
        lease_owner: &str,
        lease_until: i64,
        limit: i32,
    ) -> Result<Vec<CronTriggerPo>>;

    /// 记录执行错误
    async fn record_trigger_failure(
        &self,
        ctx: RequestContext,
        id: &str,
        error: &str,
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
    async fn query_logs(
        &self,
        ctx: RequestContext,
        query: LogQueryParam,
    ) -> Result<QueryLogsResponse>;

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

/// 统一后台进程管理（委托 pkg/process 注册中心，带 Agent scope 校验）
///
/// 同步方法：注册中心为内存结构，探活/终止为轻量系统调用，无需 async。
pub trait ProcessManager: Send + Sync {
    /// 查询单个进程（先探活刷新状态）
    fn get_process(
        &self,
        ctx: RequestContext,
        pid: u32,
    ) -> Result<crate::pkg::process::ProcessEntry>;

    /// 列出进程（Agent 调用方仅可见自己启动的）
    ///
    /// 顺序契约：最新启动在前（started_at 降序，同秒 pid 降序）。
    /// 排序由供给侧保证，调用方直接按序展示；分页可在此切页前完成。
    fn list_processes(&self, ctx: RequestContext)
    -> Result<Vec<crate::pkg::process::ProcessEntry>>;

    /// 终止进程（SIGKILL），返回是否实际执行了终止（已退出返回 false）
    fn kill_process(&self, ctx: RequestContext, pid: u32) -> Result<bool>;

    /// 进程状态详情（探活刷新后的 entry + 日志尾部 tail_lines 行，默认 20 上限 500）
    fn process_status(
        &self,
        ctx: RequestContext,
        pid: u32,
        tail_lines: Option<usize>,
    ) -> Result<ProcessStatusDetail>;
}

#[async_trait::async_trait]
impl CronManager for SystemDomainImpl {
    async fn create_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        validate_trigger(trigger)?;
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

    async fn count_triggers(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<usize> {
        self.cron_trigger_dal.count(ctx, query).await
    }

    async fn query_trigger_exec(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::cron_trigger::TriggerExecQuery,
    ) -> Result<crate::service::dao::cron_trigger::TriggerExecQueryResult> {
        self.cron_trigger_dal.query_exec(ctx, query).await
    }

    async fn query_trigger_exec_stats(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::cron_trigger::TriggerExecStatsQuery,
    ) -> Result<crate::service::dao::cron_trigger::TriggerExecStatsResult> {
        self.cron_trigger_dal.query_exec_stats(ctx, query).await
    }

    async fn query_schedules(
        &self,
        ctx: RequestContext,
        query: ScheduleQuery,
    ) -> Result<common::api::QueryScheduleResponse> {
        let owner_user_id = ctx.uid();
        if owner_user_id.is_empty() {
            bail_err!(InvalidRequest, "当前请求缺少用户上下文，无法查询日程");
        }

        let timezone = crate::config::timezone();
        let now = common::constants::utils::current_timestamp_ms();
        let (from, to) = schedule_query::resolve_range(&query, now, &timezone)?;

        let dao_query = CronTriggerQuery {
            owner_user_id: Some(owner_user_id),
            behavior: query.behavior,
            is_enabled: query.is_enabled,
            limit: Some(schedule_query::MAX_RULES),
            ..Default::default()
        };
        let triggers = self.cron_trigger_dal.list(ctx, dao_query).await?;
        let occurrences =
            schedule_query::collect_occurrences(&triggers, &query, from, to, &timezone)?;
        Ok(schedule_query::into_response(
            &query,
            from,
            to,
            timezone,
            occurrences,
        ))
    }

    async fn update_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        // 与 create_trigger 同一套校验：更新也必须拒绝非法 cron / interval<=0 /
        // 反向窗口 / 不完整的异步 Who 组。否则改坏了要等到执行时才炸。
        validate_trigger(trigger)?;

        // 时间规则是「游标 + 三选一」结构：改了表达式/间隔/一次性时刻后，
        // next_run_at 必须按新规则重算，否则下一次仍按旧规则触发，用户会觉得「改了没生效」。
        // 旧规则从库里读：调用方只交完整 PO，domain 自己判有没有动过时间字段。
        let mut updated = trigger.clone();
        if let Some(stored) = self
            .cron_trigger_dal
            .get_by_id(ctx.clone(), &trigger.id)
            .await?
        {
            if trigger_time_rule_changed(&stored, trigger) {
                let timezone = crate::config::timezone();
                updated.next_run_at = recompute_next_run_at(trigger, &timezone)?;
            }
            // Cancelled（软删 is_enabled=0）不可经更新复活，必须走 resume 显式恢复。
            if stored.is_enabled == 0 && trigger.is_enabled == 1 {
                bail_err!(
                    InvalidRequest,
                    "触发器 '{}' 已停用，请通过 resume 显式恢复",
                    trigger.name
                );
            }
        }
        self.cron_trigger_dal.update(ctx, &updated).await
    }

    async fn cancel_schedule(&self, ctx: RequestContext, trigger_id: &str) -> Result<bool> {
        let owner = ctx.uid();
        if owner.is_empty() {
            bail_err!(InvalidRequest, "当前请求缺少用户上下文，无法取消日程");
        }
        let Some(trigger) = self
            .cron_trigger_dal
            .get_by_id(ctx.clone(), trigger_id)
            .await?
        else {
            // 不存在 / 不属于自己统一 NotFound，不泄漏他人日程的存在性
            bail_err!(NotFound, "日程不存在: {}", trigger_id);
        };
        schedule_mutation::ensure_owned(&trigger, &owner)?;
        if trigger.is_enabled == 0 {
            // 幂等：已取消的直接返回，不报错（与 cancel_thinking 的 NotThinking 同理）
            return Ok(false);
        }
        self.cron_trigger_dal.delete(ctx, trigger_id).await?;
        Ok(true)
    }

    async fn update_schedule(
        &self,
        ctx: RequestContext,
        update: ScheduleUpdate,
    ) -> Result<CronTriggerPo> {
        let owner = ctx.uid();
        if owner.is_empty() {
            bail_err!(InvalidRequest, "当前请求缺少用户上下文，无法更新日程");
        }
        let mut trigger = self
            .cron_trigger_dal
            .get_by_id(ctx.clone(), &update.trigger_id)
            .await?
            .ok_or_else(|| err!(NotFound, "日程不存在: {}", update.trigger_id))?;
        schedule_mutation::ensure_owned(&trigger, &owner)?;

        schedule_mutation::apply_allowed_update(&mut trigger, &update);
        // 复用 update_trigger：它负责校验 + 时间规则变更时重算 next_run_at
        self.update_trigger(ctx.clone(), &trigger).await?;

        self.cron_trigger_dal
            .get_by_id(ctx, &update.trigger_id)
            .await?
            .ok_or_else(|| err!(NotFound, "日程更新后读取失败: {}", update.trigger_id))
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

    async fn claim_due_triggers(
        &self,
        ctx: RequestContext,
        now: i64,
        lease_owner: &str,
        lease_until: i64,
        limit: i32,
    ) -> Result<Vec<CronTriggerPo>> {
        self.cron_trigger_dal
            .claim_due(ctx, now, lease_owner, lease_until, limit)
            .await
    }

    async fn record_trigger_failure(
        &self,
        ctx: RequestContext,
        id: &str,
        error: &str,
    ) -> Result<()> {
        self.cron_trigger_dal.record_failure(ctx, id, error).await
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
    ) -> Result<QueryLogsResponse> {
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

// ==================== 系统级默认定时任务 ====================

/// 系统默认 agent_rest 触发器名（创建与升级判定共用）
const SYSTEM_AGENT_REST_NAME: &str = "系统默认-Agent 睡眠沉淀";

/// 旧版系统默认 payload（`settle_limit=10`）
///
/// 仅用于识别「用户从未改过」的存量系统触发器并做一次性升级；
/// 只要 payload 与它不完全一致（用户调过 limit / 改了 agent_id 等），一律不动。
const LEGACY_AGENT_REST_PAYLOAD: &str = r#"{"action":"agent_rest","extra":{"settle_limit":10}}"#;

/// 当前系统默认 payload
///
/// 不再人为限制 10 条：实际批量由 [`settle_memory`](crate::handlers::hr::agent::settle_memory)
/// 的上下文预算自适应决定，这里的 `settle_limit` 只是硬上限（与 `PENDING_MAX_ITEMS` 同值）。
fn agent_rest_payload() -> String {
    format!(
        r#"{{"action":"agent_rest","extra":{{"settle_limit":{}}}}}"#,
        common::constants::settle::PENDING_MAX_ITEMS
    )
}

/// 是否为「仍是旧默认 payload」的系统默认 agent_rest 触发器
///
/// 精确匹配名字 + payload，避免把用户改过的配置当成旧默认值覆盖掉。
fn is_legacy_agent_rest(trigger: &CronTriggerPo) -> bool {
    trigger.name == SYSTEM_AGENT_REST_NAME && trigger.payload == LEGACY_AGENT_REST_PAYLOAD
}

/// 确保系统级默认定时任务存在（幂等，已有同类型则跳过）
///
/// 设计决策：如果用户已有同 action（agent_rest / project_followup）的触发器
/// 则不重复添加。系统仅提供初始化默认值，用户可自行修改间隔或禁用。
///
/// 唯一的例外是**存量默认值的升级**：系统默认 agent_rest 若仍是旧版 payload
/// （`settle_limit=10`），会就地更新为新默认值 —— 否则老环境永远停在 10 条，
/// 改常量只对新装库生效。用户自定义过的 payload 不满足精确匹配，不会被覆盖。
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
    let has_tool_log_cleanup = existing
        .iter()
        .any(|t| t.payload.contains("\"tool_log_cleanup\""));
    let has_directory_reconcile = existing
        .iter()
        .any(|t| t.payload.contains("\"directory_reconcile\""));
    let has_pending_redelivery = existing
        .iter()
        .any(|t| t.payload.contains("\"pending_notification_redelivery\""));
    let has_vector_maintenance = existing
        .iter()
        .any(|t| t.payload.contains("\"vector_maintenance\""));

    // 1. agent_rest：每天凌晨 4 点执行一次睡眠沉淀（Cron 表达式，系统时区）
    if !has_agent_rest {
        let expression = "0 4 * * *"; // 每天 04:00（分 时 日 月 周）
        let timezone = crate::config::timezone();
        let next_run_at = common::schedule::next_run_at(expression, &timezone, chrono::Utc::now())?;
        let mut trigger = CronTriggerPo::new(
            common::new_id!(CronTrigger),
            SYSTEM_AGENT_REST_NAME.into(),
            TriggerType::Cron,
            next_run_at,
            Some("system".into()),
        );
        trigger.cron_expression = Some(expression.into());
        trigger.payload = agent_rest_payload();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx.clone(), &trigger).await?;
        sys_info!(
            "已创建系统级定时任务: agent_rest (cron: {} {})",
            expression,
            timezone
        );
    } else {
        // 存量升级：只动「系统默认名字 + 旧默认 payload」这一种，用户改过的配置不碰
        for trigger in existing.iter().filter(|t| is_legacy_agent_rest(t)) {
            let mut upgraded = trigger.clone();
            upgraded.payload = agent_rest_payload();
            upgraded.touch(Some("system".into()));
            cron_manager.update_trigger(ctx.clone(), &upgraded).await?;
            sys_info!(
                "已升级系统默认 agent_rest 触发器 payload: settle_limit 10 → {}",
                common::constants::settle::PENDING_MAX_ITEMS
            );
        }
    }

    // 2. project_followup：默认每 1 小时执行一次项目进度巡检
    if !has_project_followup {
        let mut trigger = CronTriggerPo::new(
            common::new_id!(CronTrigger),
            "系统默认-项目进度巡检".into(),
            TriggerType::Interval,
            common::constants::utils::current_timestamp_ms() + 3600 * 1000,
            Some("system".into()),
        );
        trigger.interval_seconds = Some(3600);
        trigger.payload = r#"{"action":"project_followup","extra":{}}"#.into();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx.clone(), &trigger).await?;
        sys_info!("已创建系统级定时任务: project_followup");
    }

    // 3. tool_log_cleanup：每天凌晨 5 点清理超期工具运行日志（① 运行时输出 TTL，
    //    保留天数读 [tool_log].retention_days；Running 进程日志受保护不删）
    if !has_tool_log_cleanup {
        let expression = "0 5 * * *"; // 每天 05:00（分 时 日 月 周）
        let timezone = crate::config::timezone();
        let next_run_at = common::schedule::next_run_at(expression, &timezone, chrono::Utc::now())?;
        let mut trigger = CronTriggerPo::new(
            common::new_id!(CronTrigger),
            "系统默认-工具日志清理".into(),
            TriggerType::Cron,
            next_run_at,
            Some("system".into()),
        );
        trigger.cron_expression = Some(expression.into());
        trigger.payload = r#"{"action":"tool_log_cleanup","extra":{}}"#.into();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx.clone(), &trigger).await?;
        sys_info!(
            "已创建系统级定时任务: tool_log_cleanup (cron: {} {})",
            expression,
            timezone
        );
    }

    // 4. directory_reconcile：每天凌晨 3 点对账组织目录（逐 Active 对端双向同步，
    //    兜底变更推送丢失/失败的场景，保证目录最终一致）
    if !has_directory_reconcile {
        let expression = "0 3 * * *"; // 每天 03:00（分 时 日 月 周）
        let timezone = crate::config::timezone();
        let next_run_at = common::schedule::next_run_at(expression, &timezone, chrono::Utc::now())?;
        let mut trigger = CronTriggerPo::new(
            common::new_id!(CronTrigger),
            "系统默认-组织目录对账".into(),
            TriggerType::Cron,
            next_run_at,
            Some("system".into()),
        );
        trigger.cron_expression = Some(expression.into());
        trigger.payload = r#"{"action":"directory_reconcile","extra":{}}"#.into();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx.clone(), &trigger).await?;
        sys_info!(
            "已创建系统级定时任务: directory_reconcile (cron: {} {})",
            expression,
            timezone
        );
    }

    // 5. pending_notification_redelivery：每 5 分钟重投「滞留 Pending」的跟进通知
    //
    // 跟进通知是项目推进的机械兜底，不该被「谁在忙」抑制（相关门禁已删除）。
    // 但投递后若接收方持续占线，AOP 退避重投会以 Discard 收场 —— 消息停在
    // Pending 且再无消费者。本触发器把「消息表 = 权威待办」这件事补完。
    if !has_pending_redelivery {
        let mut trigger = CronTriggerPo::new(
            common::new_id!(CronTrigger),
            "系统默认-滞留通知重投".into(),
            TriggerType::Interval,
            common::constants::utils::current_timestamp_ms() + 300 * 1000,
            Some("system".into()),
        );
        trigger.interval_seconds = Some(300);
        trigger.payload = r#"{"action":"pending_notification_redelivery","extra":{}}"#.into();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx.clone(), &trigger).await?;
        sys_info!("已创建系统级定时任务: pending_notification_redelivery (interval: 300s)");
    }

    // 6. vector_maintenance：每天凌晨 5:30 整理向量库（合并碎片 + 回收旧版本）
    //
    // LanceDB 每次写入都会新增版本，长期运行后 `_versions/` 会膨胀到远超真实数据；
    // 碎片不合并还会让查询退化为全表扫描。与工具日志清理错开半小时，避免同时占用磁盘。
    if !has_vector_maintenance {
        let expression = "30 5 * * *"; // 每天 05:30（分 时 日 月 周）
        let timezone = crate::config::timezone();
        let next_run_at = common::schedule::next_run_at(expression, &timezone, chrono::Utc::now())?;
        let mut trigger = CronTriggerPo::new(
            common::new_id!(CronTrigger),
            "系统默认-向量库整理".into(),
            TriggerType::Cron,
            next_run_at,
            Some("system".into()),
        );
        trigger.cron_expression = Some(expression.into());
        trigger.payload = r#"{"action":"vector_maintenance","extra":{}}"#.into();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx.clone(), &trigger).await?;
        sys_info!(
            "已创建系统级定时任务: vector_maintenance (cron: {} {})",
            expression,
            timezone
        );
    }

    Ok(())
}

/// 时间规则三要素（cron > interval > run_at）是否发生变化。
///
/// 只比这三组字段，不比 `next_run_at` 本身——那是重算结果，不是输入。
fn trigger_time_rule_changed(old: &CronTriggerPo, new: &CronTriggerPo) -> bool {
    old.cron_expression != new.cron_expression
        || old.interval_seconds != new.interval_seconds
        || old.run_at != new.run_at
        || old.trigger_type != new.trigger_type
}

/// 按当前时间规则重算下一次触发时刻（cron > interval > run_at）。
///
/// 与两个创建入口（管理端 `create_cron_trigger`、Agent `schedule_agent_task`）的
/// 优先级一致；抽成 domain 纯函数，避免更新路径各算一套导致游标漂移。
fn recompute_next_run_at(trigger: &CronTriggerPo, timezone: &str) -> Result<i64> {
    if let Some(expression) = trigger
        .cron_expression
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        return common::schedule::next_run_at(expression, timezone, chrono::Utc::now());
    }
    if let Some(interval) = trigger.interval_seconds {
        return common::schedule::interval_next_run_at(
            common::constants::utils::current_timestamp_ms(),
            interval,
        );
    }
    trigger.run_at.ok_or_else(|| {
        err!(
            InvalidRequest,
            "trigger '{}' 缺少有效时间规则，无法重算 next_run_at",
            trigger.name
        )
    })
}

/// 触发器条件必填校验（A-设计 §7），落 Domain 层（B-方案 P2 范围）。
///
/// 分两档：
/// - `SyncMaintenance`（系统 seed / 存量 6 条）：只校验时间规则，Who 组允许为空——
///   系统触发器没有归属用户与目标，历史数据本来就是空，收紧会打断 seed。
/// - 异步模板（`AsyncNotify` / `AsyncAgentWakeup`）：Who 组必填 + 窗口 + 时间规则。
///   其中 `org_id` **可空**：日程优先绑定用户 / Agent，系统日程缺省即本地组织
///   （本地部署通常只有一个组织，远程组织走独立实例）；`owner_user_id`、
///   `creator_type`、`creator_id` 必填；`AsyncAgentWakeup` 还必须有 `agent_id`
///   （负责 Agent），否则到点无人可唤醒。
///
/// ⚠️ 目标 Agent 的「同组织」校验**不在这里**：`agents` 表无 `organization_id`、
/// 全库无 Agent↔组织关联表（见 `service/domain/runtime/cancel.rs` 头注释），
/// 租户门做不了。存在性 + 状态校验在 `schedule_agent_task` handler 内完成。
fn validate_trigger(trigger: &CronTriggerPo) -> Result<()> {
    // 时间规则：Schedule 三选一，其余按类型必填
    let has_time = match trigger.trigger_type {
        TriggerType::Once => trigger.run_at.is_some(),
        TriggerType::Cron => trigger
            .cron_expression
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty()),
        TriggerType::Interval => trigger.interval_seconds.is_some_and(|v| v > 0),
        TriggerType::Schedule => {
            trigger
                .cron_expression
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty())
                || trigger.interval_seconds.is_some_and(|v| v > 0)
                || trigger.run_at.is_some()
        }
    };
    if !has_time {
        return Err(err!(
            InvalidRequest,
            "trigger '{}' 缺少有效时间规则（{:?}）",
            trigger.name,
            trigger.trigger_type
        ));
    }

    // 窗口：end_at 必须晚于 start_at
    if let (Some(start), Some(end)) = (trigger.start_at, trigger.end_at)
        && end <= start
    {
        return Err(err!(
            InvalidRequest,
            "trigger '{}' 的 end_at 必须大于 start_at",
            trigger.name
        ));
    }

    // 次数：0 或负数会让规则在创建瞬间就「已耗尽」，必须挡在入口
    if trigger.max_runs.is_some_and(|max| max <= 0) {
        return Err(err!(
            InvalidRequest,
            "trigger '{}' 的 max_runs 必须为正数",
            trigger.name
        ));
    }

    if trigger.behavior == TriggerBehavior::SyncMaintenance {
        return Ok(());
    }

    // 异步日程：归属用户 + 创建者必填（推送面 = owner_user_id）。
    // `org_id` 不在必填项内——见上方文档注释。
    let who_missing = trigger.owner_user_id.is_none()
        || trigger.creator_type.is_none()
        || trigger.creator_id.is_none();
    if who_missing {
        return Err(err!(
            InvalidRequest,
            "trigger '{}' 使用异步模板但 Who 组字段不完整",
            trigger.name
        ));
    }

    // 唤醒模板必须有负责 Agent：没有它，到点只能记一条告警然后跳过。
    if trigger.behavior == TriggerBehavior::AsyncAgentWakeup
        && trigger.agent_id.as_deref().is_none_or(str::is_empty)
    {
        return Err(err!(
            InvalidRequest,
            "trigger '{}' 使用唤醒模板但未指定负责 Agent",
            trigger.name
        ));
    }

    // 走到这里的一定是异步模板（SyncMaintenance 已在上方提前放行）。
    //
    // ⚠️ `Schedule + SyncMaintenance` 是**允许**的：管理接口 `create_cron_trigger`
    // 可手动建「系统调度规则」；只有 Agent 工具 `schedule_agent_task` 显式拒绝同步
    // 模板（见其 behavior 白名单），两条入口的约束不同，不要在这里收紧。
    Ok(())
}

#[cfg(test)]
mod legacy_payload_tests {
    use super::*;

    fn trigger(name: &str, payload: &str) -> CronTriggerPo {
        let mut t = CronTriggerPo::new(
            common::new_id!(CronTrigger),
            name.into(),
            TriggerType::Cron,
            common::constants::utils::current_timestamp_ms(),
            Some("system".into()),
        );
        t.payload = payload.into();
        t
    }

    #[test]
    fn legacy_default_is_recognized() {
        // 仅「系统默认名字 + 旧默认 payload」的精确组合会被升级
        assert!(is_legacy_agent_rest(&trigger(
            SYSTEM_AGENT_REST_NAME,
            LEGACY_AGENT_REST_PAYLOAD
        )));
    }

    #[test]
    fn customized_payload_is_not_touched() {
        // 用户调过 limit：payload 与旧默认不一致，不得覆盖
        assert!(!is_legacy_agent_rest(&trigger(
            SYSTEM_AGENT_REST_NAME,
            r#"{"action":"agent_rest","extra":{"settle_limit":5}}"#
        )));
        // 同名但 payload 结构不同（如指定了 agent_id）
        assert!(!is_legacy_agent_rest(&trigger(
            SYSTEM_AGENT_REST_NAME,
            r#"{"action":"agent_rest","extra":{"agent_id":"a1","settle_limit":10}}"#
        )));
        // 用户自定义名字即使是旧默认 payload 也不动
        assert!(!is_legacy_agent_rest(&trigger(
            "我的沉淀任务",
            LEGACY_AGENT_REST_PAYLOAD
        )));
    }
}

#[cfg(test)]
mod trigger_validation_tests {
    use super::*;
    use chrono::Timelike;
    use common::enums::CreatorType;

    fn schedule() -> CronTriggerPo {
        let mut t = CronTriggerPo::new(
            common::new_id!(CronTrigger),
            "日程".into(),
            TriggerType::Schedule,
            common::constants::utils::current_timestamp_ms() + 60_000,
            Some("agent-1".into()),
        );
        t.run_at = Some(common::constants::utils::current_timestamp_ms() + 60_000);
        t.behavior = TriggerBehavior::AsyncAgentWakeup;
        t.org_id = Some("org-1".into());
        t.owner_user_id = Some("user-1".into());
        t.creator_type = Some(CreatorType::Agent.to_i32());
        t.creator_id = Some("agent-1".into());
        t.agent_id = Some("agent-1".into());
        t
    }

    #[test]
    fn system_maintenance_allows_empty_who() {
        let mut t = CronTriggerPo::new(
            common::new_id!(CronTrigger),
            "系统默认".into(),
            TriggerType::Cron,
            1000,
            Some("system".into()),
        );
        t.cron_expression = Some("0 4 * * *".into());
        assert!(validate_trigger(&t).is_ok());
    }

    #[test]
    fn async_schedule_requires_who_group() {
        let mut t = schedule();
        t.owner_user_id = None;
        assert!(validate_trigger(&t).is_err());
    }

    #[test]
    fn async_schedule_allows_missing_org() {
        // org_id 可空：系统日程缺省即本地组织，不因缺 org 拒绝创建
        let mut t = schedule();
        t.org_id = None;
        assert!(validate_trigger(&t).is_ok());
    }

    #[test]
    fn wakeup_requires_responsible_agent() {
        let mut t = schedule();
        t.agent_id = None;
        assert!(validate_trigger(&t).is_err());
        // 通知模板不需要负责 Agent（由系统发出提醒）
        t.behavior = TriggerBehavior::AsyncNotify;
        assert!(validate_trigger(&t).is_ok());
    }

    #[test]
    fn rejects_non_positive_max_runs() {
        let mut t = schedule();
        t.max_runs = Some(0);
        assert!(validate_trigger(&t).is_err());
        t.max_runs = Some(1);
        assert!(validate_trigger(&t).is_ok());
    }

    #[test]
    fn schedule_requires_time_rule() {
        let mut t = schedule();
        t.run_at = None;
        assert!(validate_trigger(&t).is_err());
    }

    #[test]
    fn rejects_inverted_window() {
        let mut t = schedule();
        t.start_at = Some(2000);
        t.end_at = Some(1000);
        assert!(validate_trigger(&t).is_err());
    }

    #[test]
    fn valid_schedule_passes() {
        assert!(validate_trigger(&schedule()).is_ok());
    }

    #[test]
    fn time_rule_change_detected() {
        let base = schedule();
        let mut changed = base.clone();
        changed.cron_expression = Some("0 9 * * *".into());
        assert!(trigger_time_rule_changed(&base, &changed));
        changed = base.clone();
        changed.description = "只改描述".into();
        assert!(!trigger_time_rule_changed(&base, &changed));
    }

    #[test]
    fn recompute_follows_cron_then_interval_then_run_at() {
        let mut t = schedule();
        let now = common::constants::utils::current_timestamp_ms();
        // 三者都在时按 cron 优先：结果是未来的整点（分钟/秒为 0）
        t.cron_expression = Some("0 9 * * *".into());
        t.interval_seconds = Some(60);
        let recomputed = recompute_next_run_at(&t, "Asia/Shanghai").unwrap();
        assert!(recomputed > now);
        let local = chrono::DateTime::from_timestamp_millis(recomputed)
            .unwrap()
            .with_timezone(&chrono_tz::Asia::Shanghai);
        assert_eq!((local.hour(), local.minute(), local.second()), (9, 0, 0));
        // 去掉 cron 后按 interval：约等于 now + 60s（不卡具体毫秒）
        t.cron_expression = None;
        let recomputed = recompute_next_run_at(&t, "Asia/Shanghai").unwrap();
        assert!((now + 59_000..=now + 61_000).contains(&recomputed));
        // 只留 run_at 时原样返回
        t.interval_seconds = None;
        assert_eq!(
            recompute_next_run_at(&t, "Asia/Shanghai").unwrap(),
            t.run_at.unwrap()
        );
        // 全无则报错
        t.run_at = None;
        assert!(recompute_next_run_at(&t, "Asia/Shanghai").is_err());
    }
}
