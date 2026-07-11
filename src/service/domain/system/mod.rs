//! System Domain 模块
//!
//! 系统领域，管理：
//! - CronTrigger - 定时触发器

use common::error::Result;
use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::service::dal::cron_trigger as cron_trigger_dal;
use crate::service::dal::cron_trigger::CronTriggerDal;
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
    let _ = SYSTEM_DOMAIN.set(new(cron_trigger_dal::dal()));
}

/// 创建 System Domain 实例（测试可注入隔离依赖）。
pub fn new(
    cron_trigger_dal: Arc<dyn CronTriggerDal>,
) -> Arc<dyn SystemDomain> {
    Arc::new(SystemDomainImpl::new(cron_trigger_dal))
}

// ==================== 实现 ====================

/// System Domain 实现
///
/// 聚合所有系统子功能实现
struct SystemDomainImpl {
    cron_trigger_dal: Arc<dyn CronTriggerDal>,
}

impl SystemDomainImpl {
    /// 创建 Domain 实例
    fn new(
        cron_trigger_dal: Arc<dyn CronTriggerDal>,
    ) -> Self {
        Self {
            cron_trigger_dal,
        }
    }
}

impl SystemDomain for SystemDomainImpl {
    fn cron_manager(&self) -> &dyn CronManager {
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
