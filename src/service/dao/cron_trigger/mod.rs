
//! Cron Trigger DAO layer
//! DAO 只负责 CronTriggerPo 持久化。

use common::error::Result;
use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::enums::TriggerType;

/// Cron Trigger 查询参数。
#[derive(Debug, Clone, Default)]
pub struct CronTriggerQuery {
    pub trigger_type: Option<TriggerType>,
    pub is_enabled: Option<bool>,
    pub limit: Option<usize>,
}

/// Cron Trigger DAO trait。
#[async_trait]
pub trait CronTriggerDao: Send + Sync + std::fmt::Debug {
    /// 创建触发器
    async fn create(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;

    /// 根据 ID 获取触发器
    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>>;

    /// 通用查询
    async fn list(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<Vec<CronTriggerPo>>;

    /// 更新触发器
    async fn update(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;

    /// 删除触发器（软删除，is_enabled = 0）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 获取所有到期的触发器（next_run_at <= now AND is_enabled = 1）
    async fn list_due(&self, ctx: RequestContext, now: i64, limit: i32) -> Result<Vec<CronTriggerPo>>;

    /// 更新下次执行时间
    async fn update_next_run_at(&self, ctx: RequestContext, id: &str, next_run_at: i64, last_run_at: i64) -> Result<()>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
pub(crate) mod sqlite_test;
