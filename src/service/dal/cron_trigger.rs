//! CronTrigger DAL 模块
//!
//! 职责：CronTrigger 领域的数据访问层，封装 CronTriggerDao 提供统一的业务接口

use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::service::dao::cron_trigger;
use crate::service::dao::cron_trigger::{
    CronTriggerDao, CronTriggerQuery, TriggerExecQuery, TriggerExecQueryResult,
    TriggerExecStatsDao, TriggerExecStatsQuery, TriggerExecStatsResult,
};
use common::enums::TriggerType;
use common::error::{Error, Result, err};
use common::schedule::effective_type;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static CRON_TRIGGER_DAL: OnceLock<Arc<dyn CronTriggerDal + Send + Sync>> = OnceLock::new();

/// 获取 CronTrigger DAL 单例
pub fn dal() -> Arc<dyn CronTriggerDal + Send + Sync> {
    CRON_TRIGGER_DAL.get().cloned().unwrap()
}

/// 初始化 CronTrigger DAL
pub fn init() {
    let _ = CRON_TRIGGER_DAL.set(new(
        cron_trigger::dao(),
        // 统计库与 SQLite 触发器库是两套存储，但都经构造注入进 DAL；
        // 未注册（如测试组合根）时为 None，exec 查询返回明确错误而非 panic。
        cron_trigger::stats::stats_dao(),
    ));
}

/// 创建 CronTrigger DAL（返回 trait 对象）
pub fn new(
    cron_trigger_dao: Arc<dyn CronTriggerDao + Send + Sync>,
    exec_stats_dao: Option<Arc<dyn TriggerExecStatsDao>>,
) -> Arc<dyn CronTriggerDal + Send + Sync> {
    Arc::new(CronTriggerDalImpl {
        cron_trigger_dao,
        exec_stats_dao,
    })
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
    async fn list(
        &self,
        ctx: RequestContext,
        query: CronTriggerQuery,
    ) -> Result<Vec<CronTriggerPo>>;

    /// 按同一套条件统计总数（不含分页）
    async fn count(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<usize>;

    /// 更新
    async fn update(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;

    /// 删除（软删除）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 暂停
    async fn pause(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 恢复
    async fn resume(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 原子领取到期触发器（短租约）
    async fn claim_due(
        &self,
        ctx: RequestContext,
        now: i64,
        lease_owner: &str,
        lease_until: i64,
        limit: i32,
    ) -> Result<Vec<CronTriggerPo>>;

    /// 按 id 原子领取单条（秒级派发的逐个领取路径）
    async fn claim_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
        now: i64,
        lease_owner: &str,
        lease_until: i64,
    ) -> Result<Option<CronTriggerPo>>;

    /// 计算并更新下次执行时间（含清租约 + 累计次数）
    ///
    /// `fired_at` 是规则触发时刻（时刻型与 `last_run_at` 的基准），
    /// `finished_at` 是业务完成时刻（间隔型的基准：跑完再等 N）。
    async fn mark_executed(
        &self,
        ctx: RequestContext,
        id: &str,
        fired_at: i64,
        finished_at: i64,
    ) -> Result<()>;

    /// 记录执行错误（不清租约）
    async fn record_failure(&self, ctx: RequestContext, id: &str, error: &str) -> Result<()>;

    /// 查询执行记录（DuckDB `trigger_exec_events`，只读）
    ///
    /// 统计 DAO 与 `CronTriggerDao` 一样由构造注入（组合根注入单例，测试可传 None），
    /// 不在方法内直接取全局单例。
    async fn query_exec(
        &self,
        ctx: RequestContext,
        query: TriggerExecQuery,
    ) -> Result<TriggerExecQueryResult>;

    /// 查询执行统计概览与时序分布（DuckDB，只读）
    async fn query_exec_stats(
        &self,
        ctx: RequestContext,
        query: TriggerExecStatsQuery,
    ) -> Result<TriggerExecStatsResult>;
}

// ==================== DAL 实现 ====================

/// CronTrigger DAL 实现
struct CronTriggerDalImpl {
    cron_trigger_dao: Arc<dyn CronTriggerDao + Send + Sync>,
    /// 执行统计 DAO（DuckDB）；测试组合根可为 None
    exec_stats_dao: Option<Arc<dyn TriggerExecStatsDao>>,
}

#[async_trait::async_trait]
impl CronTriggerDal for CronTriggerDalImpl {
    async fn create(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        self.cron_trigger_dao.create(ctx, trigger).await
    }

    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>> {
        self.cron_trigger_dao.get_by_id(ctx, id).await
    }

    async fn list(
        &self,
        ctx: RequestContext,
        query: CronTriggerQuery,
    ) -> Result<Vec<CronTriggerPo>> {
        self.cron_trigger_dao.list(ctx, query).await
    }

    async fn count(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<usize> {
        self.cron_trigger_dao.count(ctx, query).await
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

    async fn claim_due(
        &self,
        ctx: RequestContext,
        now: i64,
        lease_owner: &str,
        lease_until: i64,
        limit: i32,
    ) -> Result<Vec<CronTriggerPo>> {
        self.cron_trigger_dao
            .claim_due(ctx, now, lease_owner, lease_until, limit)
            .await
    }

    async fn claim_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
        now: i64,
        lease_owner: &str,
        lease_until: i64,
    ) -> Result<Option<CronTriggerPo>> {
        self.cron_trigger_dao
            .claim_by_id(ctx, id, now, lease_owner, lease_until)
            .await
    }

    async fn mark_executed(
        &self,
        ctx: RequestContext,
        id: &str,
        fired_at: i64,
        finished_at: i64,
    ) -> Result<()> {
        let trigger = self
            .cron_trigger_dao
            .get_by_id(ctx.clone(), id)
            .await?
            .ok_or_else(|| err!(ResourceNotFound, "Trigger not found: {}", id))?;

        // 计算下一次触发时间。时刻型按规则点推进，间隔型按业务完成时刻推进。
        let timezone = crate::config::timezone();
        let next_run_at = next_run_at_after(&trigger, &timezone, fired_at, finished_at)?;

        // 终态：一次性 / 本次执行后次数用尽 / 下次已越过生效窗口终点。
        //
        // - 次数：`total_runs` 尚未含本次，所以判「本次之后是否耗尽」；
        // - 窗口：`end_at` 是闭区间上界（`within_window` / SQL / 展开都用 `<=`），
        //   只有 `next > end` 才越界；用 `>=` 会把窗口内最后一点跳过。
        let completed = next_run_at.is_none()
            || trigger.exhausts_after_current_run()
            || trigger
                .end_at
                .is_some_and(|end| next_run_at.is_some_and(|next| next > end));

        if completed {
            // 终态收尾走专用原子语句：通用 update 不再触碰运行时列，
            // 否则并发「读-改-写」会把租约与累计次数覆盖掉。
            self.cron_trigger_dao.complete_run(ctx, id, fired_at).await
        } else {
            self.cron_trigger_dao
                .update_next_run_at(
                    ctx,
                    id,
                    next_run_at.expect("completed=false 时 next_run_at 必有值"),
                    // `last_run_at` 记触发时刻，保持「上次触发」语义；
                    // 只有 Interval 的 `next_run_at` 以完成时刻为基准。
                    fired_at,
                )
                .await
        }
    }

    async fn record_failure(&self, ctx: RequestContext, id: &str, error: &str) -> Result<()> {
        self.cron_trigger_dao.record_failure(ctx, id, error).await
    }

    async fn query_exec(
        &self,
        ctx: RequestContext,
        query: TriggerExecQuery,
    ) -> Result<TriggerExecQueryResult> {
        let dao = self.exec_stats_dao.as_ref().ok_or_else(|| {
            Error::internal("trigger exec stats DAO not initialized for CronTrigger DAL")
        })?;
        dao.query(ctx, query).await
    }

    async fn query_exec_stats(
        &self,
        ctx: RequestContext,
        query: TriggerExecStatsQuery,
    ) -> Result<TriggerExecStatsResult> {
        let dao = self.exec_stats_dao.as_ref().ok_or_else(|| {
            Error::internal("trigger exec stats DAO not initialized for CronTrigger DAL")
        })?;
        dao.query_stats(ctx, query).await
    }
}

/// 本次执行后的下次触发时刻。
///
/// - 时刻型（Once / Cron）由规则决定：基准是 `fired_at`，不受业务耗时影响；
/// - 间隔型语义是「跑完再等 N」：基准是 `finished_at`，执行耗时不再吃掉空档；
/// - `Schedule` 只是形态标签，时间规则由三选一字段给出，判定复用 `effective_type`。
fn next_run_at_after(
    trigger: &CronTriggerPo,
    timezone: &str,
    fired_at: i64,
    finished_at: i64,
) -> Result<Option<i64>> {
    let rule = match trigger.trigger_type {
        TriggerType::Schedule => {
            effective_type(trigger.cron_expression.as_deref(), trigger.interval_seconds)
        }
        other => other,
    };

    match rule {
        TriggerType::Once => Ok(None), // 一次性：执行后禁用
        TriggerType::Interval => {
            let interval = trigger.interval_seconds.ok_or_else(|| {
                err!(
                    InvalidRequest,
                    "Interval trigger missing interval_seconds: {}",
                    trigger.id
                )
            })?;
            // 与两个创建入口同一套受检乘加：interval<=0 或极端值直接报错，
            // 不能 wrap 成过去时间（那会让调度器下一轮立刻再领一次，无限空转）。
            Ok(Some(common::schedule::interval_next_run_at(
                finished_at,
                interval,
            )?))
        }
        TriggerType::Cron => {
            let expression = trigger.cron_expression.as_deref().ok_or_else(|| {
                err!(
                    InvalidRequest,
                    "Cron trigger missing cron_expression: {}",
                    trigger.id
                )
            })?;
            let from = chrono::DateTime::<chrono::Utc>::from_timestamp(fired_at / 1000, 0)
                .unwrap_or_else(chrono::Utc::now);
            Ok(Some(common::schedule::next_run_at(
                expression, timezone, from,
            )?))
        }
        TriggerType::Schedule => unreachable!("Schedule 已被归一为具体时间规则"),
    }
}

/// Schedule 形态的时间规则三选一：cron > interval > run_at（Once 语义）。
///
/// 若三者皆无，回落 Once 让记录禁用而不是无限重触发（防御僵尸记录）。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schedule_time_rule_priority() {
        // 判定已下沉 common::schedule::effective_type，这里守住 DAL 调用口径
        assert_eq!(effective_type(None, None), TriggerType::Once);
        assert_eq!(effective_type(None, Some(60)), TriggerType::Interval);
        assert_eq!(
            effective_type(Some("0 4 * * *"), Some(60)),
            TriggerType::Cron
        );
    }

    #[test]
    fn interval_next_run_starts_from_finished_at() {
        let mut trigger = CronTriggerPo::new(
            "t".into(),
            "interval".into(),
            TriggerType::Interval,
            0,
            None,
        );
        trigger.interval_seconds = Some(300);

        let fired_at = 1_000;
        let finished_at = 121_000; // 业务耗时 2 分钟
        assert_eq!(
            next_run_at_after(&trigger, "UTC", fired_at, finished_at).unwrap(),
            Some(421_000)
        );
        // 完成时刻不变、触发时刻改变 → 下次时间不受影响
        assert_eq!(
            next_run_at_after(&trigger, "UTC", 999_000, finished_at).unwrap(),
            Some(421_000)
        );
    }

    #[test]
    fn cron_next_run_ignores_finished_at() {
        let mut trigger = CronTriggerPo::new("t".into(), "cron".into(), TriggerType::Cron, 0, None);
        trigger.cron_expression = Some("*/5 * * * *".into());

        let fired_at = 1_700_000_000_000;
        let fired_basis = next_run_at_after(&trigger, "UTC", fired_at, fired_at).unwrap();
        let finished_basis =
            next_run_at_after(&trigger, "UTC", fired_at, fired_at + 600_000).unwrap();
        assert!(fired_basis.is_some());
        assert_eq!(fired_basis, finished_basis);
    }

    #[test]
    fn once_has_no_next_run() {
        let trigger = CronTriggerPo::new("t".into(), "once".into(), TriggerType::Once, 0, None);
        assert_eq!(
            next_run_at_after(&trigger, "UTC", 1_000, 121_000).unwrap(),
            None
        );
    }
}
