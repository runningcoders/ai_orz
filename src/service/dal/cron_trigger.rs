//! CronTrigger DAL 模块
//!
//! 职责：CronTrigger 领域的数据访问层，封装 CronTriggerDao 提供统一的业务接口

use common::error::{bail_err, err, Result};
use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::service::dao::cron_trigger;
use crate::service::dao::cron_trigger::{CronTriggerDao, CronTriggerQuery};
use common::enums::TriggerType;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static CRON_TRIGGER_DAL: OnceLock<Arc<dyn CronTriggerDal + Send + Sync>> = OnceLock::new();

/// 获取 CronTrigger DAL 单例
pub fn dal() -> Arc<dyn CronTriggerDal + Send + Sync> {
    CRON_TRIGGER_DAL.get().cloned().unwrap()
}

/// 初始化 CronTrigger DAL
pub fn init() {
    let _ = CRON_TRIGGER_DAL.set(new(cron_trigger::dao()));
}

/// 创建 CronTrigger DAL（返回 trait 对象）
pub fn new(
    cron_trigger_dao: Arc<dyn CronTriggerDao + Send + Sync>,
) -> Arc<dyn CronTriggerDal + Send + Sync> {
    Arc::new(CronTriggerDalImpl { cron_trigger_dao })
}

// ==================== DAL 接口 ====================

/// CronTrigger DAL 接口
#[async_trait::async_trait]
pub trait CronTriggerDal: Send + Sync {
    /// 创建触发器
    async fn create(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;

    /// 根据 ID 获取
    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>>;

    /// 列表查询
    async fn list(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<Vec<CronTriggerPo>>;

    /// 更新
    async fn update(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;

    /// 删除（软删除）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 暂停
    async fn pause(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 恢复
    async fn resume(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 获取到期触发器
    async fn list_due(&self, ctx: RequestContext, now: i64, limit: i32) -> Result<Vec<CronTriggerPo>>;

    /// 计算并更新下次执行时间
    async fn mark_executed(&self, ctx: RequestContext, id: &str, executed_at: i64) -> Result<()>;
}

// ==================== DAL 实现 ====================

/// CronTrigger DAL 实现
struct CronTriggerDalImpl {
    cron_trigger_dao: Arc<dyn CronTriggerDao + Send + Sync>,
}

#[async_trait::async_trait]
impl CronTriggerDal for CronTriggerDalImpl {
    async fn create(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        self.cron_trigger_dao.create(ctx, trigger).await
    }

    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>> {
        self.cron_trigger_dao.get_by_id(ctx, id).await
    }

    async fn list(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<Vec<CronTriggerPo>> {
        self.cron_trigger_dao.list(ctx, query).await
    }

    async fn update(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        self.cron_trigger_dao.update(ctx, trigger).await
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.cron_trigger_dao.delete(ctx, id).await
    }

    async fn pause(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let mut trigger = self
            .cron_trigger_dao
            .get_by_id(ctx.clone(), id)
            .await?
            .ok_or_else(|| err!(ResourceNotFound, "Trigger not found: {}", id))?;
        trigger.is_enabled = 0;
        trigger.touch(Some(ctx.uid()));
        self.cron_trigger_dao.update(ctx, &trigger).await
    }

    async fn resume(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let mut trigger = self
            .cron_trigger_dao
            .get_by_id(ctx.clone(), id)
            .await?
            .ok_or_else(|| err!(ResourceNotFound, "Trigger not found: {}", id))?;
        trigger.is_enabled = 1;
        trigger.touch(Some(ctx.uid()));
        self.cron_trigger_dao.update(ctx, &trigger).await
    }

    async fn list_due(&self, ctx: RequestContext, now: i64, limit: i32) -> Result<Vec<CronTriggerPo>> {
        self.cron_trigger_dao.list_due(ctx, now, limit).await
    }

    async fn mark_executed(&self, ctx: RequestContext, id: &str, executed_at: i64) -> Result<()> {
        let trigger = self
            .cron_trigger_dao
            .get_by_id(ctx.clone(), id)
            .await?
            .ok_or_else(|| err!(ResourceNotFound, "Trigger not found: {}", id))?;

        match trigger.trigger_type {
            TriggerType::Once => {
                let mut trigger = trigger;
                trigger.is_enabled = 0;
                trigger.last_run_at = Some(executed_at);
                trigger.touch(Some(ctx.uid()));
                self.cron_trigger_dao.update(ctx, &trigger).await
            }
            TriggerType::Interval => {
                let interval = trigger.interval_seconds.ok_or_else(|| {
                    err!(
                        InvalidRequest,
                        "Interval trigger missing interval_seconds: {}",
                        id
                    )
                })?;
                let next_run_at = executed_at + interval;
                self.cron_trigger_dao
                    .update_next_run_at(ctx, id, next_run_at, executed_at)
                    .await
            }
            TriggerType::Cron => {
                bail_err!(
                    UnsupportedOperation,
                    "Cron expression parsing not yet implemented"
                );
            }
        }
    }
}
