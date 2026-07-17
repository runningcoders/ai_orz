//! System Domain 模块
//!
//! 系统领域，管理：
//! - CronTrigger - 定时触发器
//! - Backup - 数据备份与恢复

use common::error::Result;
use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::service::dal::backup as backup_dal;
use crate::service::dal::backup::{BackupDal, BackupInfo};
use crate::service::dal::cron_trigger as cron_trigger_dal;
use crate::service::dal::cron_trigger::CronTriggerDal;
use crate::service::dal::log_query as log_query_dal;
use crate::service::dal::log_query::{LogQuery as LogQueryParam, LogPageResult, LogQueryDal};
use crate::service::dao::cron_trigger::CronTriggerQuery;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static SYSTEM_DOMAIN: OnceLock<Arc<dyn SystemDomain>> = OnceLock::new();

/// 获取 System Domain 单例
pub fn domain() -> Arc<dyn SystemDomain> {
    SYSTEM_DOMAIN.get().cloned().unwrap()
}

/// 初始化 System Domain
pub fn init() {
    let _ = SYSTEM_DOMAIN.set(new(
        cron_trigger_dal::dal(),
        backup_dal::dal(),
        log_query_dal::dal(),
    ));
}

/// 创建 System Domain 实例（测试可注入隔离依赖）。
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

// ==================== 实现 ====================

/// System Domain 实现
///
/// 聚合所有系统子功能实现
struct SystemDomainImpl {
    cron_trigger_dal: Arc<dyn CronTriggerDal>,
    backup_dal: Arc<dyn BackupDal + Send + Sync>,
    log_query_dal: Arc<dyn LogQueryDal + Send + Sync>,
}

impl SystemDomainImpl {
    /// 创建 Domain 实例
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
}

// ==================== traits 定义 ====================

/// System Domain 总 trait
///
/// 聚合系统领域所有子功能 trait
pub trait SystemDomain: Send + Sync {
    /// Cron 管理能力
    fn cron_manager(&self) -> &dyn CronManager;

    /// Backup 管理能力
    fn backup_manager(&self) -> &dyn BackupManager;

    /// 日志查询能力
    fn log_query(&self) -> &dyn LogQuery;
}

/// Cron 管理 trait
///
/// 定义 Cron 触发器相关的业务接口
#[async_trait::async_trait]
pub trait CronManager: Send + Sync {
    /// 创建触发器
    async fn create_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;

    /// 获取触发器
    async fn get_trigger(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>>;

    /// 列表查询
    async fn list_triggers(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<Vec<CronTriggerPo>>;

    /// 更新触发器
    async fn update_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;

    /// 删除触发器
    async fn delete_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 暂停
    async fn pause_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 恢复
    async fn resume_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 获取到期触发器（供 CronScheduler 调用）
    async fn list_due_triggers(&self, ctx: RequestContext, now: i64, limit: i32) -> Result<Vec<CronTriggerPo>>;

    /// 标记已执行（供消费者调用，更新下次执行时间）
    async fn mark_trigger_executed(&self, ctx: RequestContext, id: &str, executed_at: i64) -> Result<()>;
}

/// Backup 管理 trait
///
/// 定义数据备份与恢复相关的业务接口
#[async_trait::async_trait]
pub trait BackupManager: Send + Sync {
    /// 创建一份新备份，返回其元信息
    async fn create_backup(&self, ctx: RequestContext) -> Result<BackupInfo>;

    /// 列出所有备份（按 version 降序）
    async fn list_backups(&self, ctx: RequestContext) -> Result<Vec<BackupInfo>>;

    /// 删除指定版本的备份
    async fn delete_backup(&self, ctx: RequestContext, version: u64) -> Result<()>;

    /// 生成指定版本的恢复脚本（bash）
    async fn generate_restore_script(&self, ctx: RequestContext, version: u64) -> Result<String>;
}

/// 日志查询 trait
///
/// 定义日志查询相关的业务接口
#[async_trait::async_trait]
pub trait LogQuery: Send + Sync {
    /// 查询日志，返回分页结果（按时间倒序，最新的在前）
    async fn query_logs(&self, ctx: RequestContext, query: LogQueryParam) -> Result<LogPageResult>;
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
