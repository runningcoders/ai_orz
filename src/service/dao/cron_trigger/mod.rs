//! Cron Trigger DAO 层

use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::enums::{TriggerBehavior, TriggerType};
use common::error::Result;

/// Cron Trigger 查询参数。
#[derive(Debug, Clone, Default)]
pub struct CronTriggerQuery {
    pub trigger_type: Option<TriggerType>,
    pub behavior: Option<TriggerBehavior>,
    pub org_id: Option<String>,
    pub owner_user_id: Option<String>,
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
    async fn list(
        &self,
        ctx: RequestContext,
        query: CronTriggerQuery,
    ) -> Result<Vec<CronTriggerPo>>;

    /// 更新触发器配置（**只写配置列**）。
    ///
    /// 运行时列（`total_runs` / `last_run_at` / `last_error` / 租约两列）刻意不在
    /// 本方法内：管理端「读 → 改 → 写」与执行并发时，全列覆盖会把租约清空、
    /// 把累计次数回退，导致同一触发器被重复领取。运行时状态只能经
    /// `claim_due` / `update_next_run_at` / `complete_run` / `record_failure`
    /// 这几条专用原子语句改写。
    async fn update(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;

    /// 删除触发器（软删除，is_enabled = 0）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 原子领取到期触发器（短租约）。
    ///
    /// 只返回成功写入租约的行：`UPDATE ... WHERE lease_until IS NULL OR <= now`，
    /// `rows_affected == 1` 才 emit。多实例并发下保证「同一时刻只有一个实例持有到期触发」。
    async fn claim_due(
        &self,
        ctx: RequestContext,
        now: i64,
        lease_owner: &str,
        lease_until: i64,
        limit: i32,
    ) -> Result<Vec<CronTriggerPo>>;

    /// 更新下次执行时间 + 清租约 + 累计次数（成功收尾）
    async fn update_next_run_at(
        &self,
        ctx: RequestContext,
        id: &str,
        next_run_at: i64,
        last_run_at: i64,
    ) -> Result<()>;

    /// 记录执行错误（不清租约：租约自然过期即下轮重捞）
    async fn record_failure(&self, ctx: RequestContext, id: &str, error: &str) -> Result<()>;

    /// 终态收尾：累计次数 +1、禁用、清租约、记最后一次执行时刻。
    ///
    /// 一次性 / 次数用尽 / 越过窗口终点都走这里。必须是单条原子语句：
    /// `total_runs = total_runs + 1` 在库里自增，不能读出来加一再写回，
    /// 否则与并发执行互相覆盖计数。
    async fn complete_run(&self, ctx: RequestContext, id: &str, executed_at: i64) -> Result<()>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
pub(crate) mod sqlite_test;
