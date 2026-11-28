//! SQLite implementation of Cron Trigger DAO

use super::{CronTriggerDao, CronTriggerQuery};
use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static DAO_INSTANCE: OnceLock<Arc<dyn CronTriggerDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 Cron Trigger DAO 实例（用于测试）
pub fn new() -> Arc<dyn CronTriggerDao + Send + Sync> {
    Arc::new(CronTriggerDaoSqliteImpl {})
}

/// Get the singleton Cron Trigger DAO instance
pub fn dao() -> Arc<dyn CronTriggerDao + Send + Sync> {
    DAO_INSTANCE
        .get()
        .expect("Cron Trigger DAO not initialized")
        .clone()
}

/// Initialize the Cron Trigger DAO
pub fn init() {
    let _ = DAO_INSTANCE.set(new());
}

#[derive(Debug)]
struct CronTriggerDaoSqliteImpl;

// 统一列清单：所有 SELECT 共用，避免 5 处 SQL 漂移。
// ⚠️ 顺序必须与 CronTriggerPo 字段顺序一致（sqlx FromRow 按列名映射，但保持整齐便于核对）。
const TRIGGER_COLUMNS: &str = concat!(
    "id, name, trigger_type, cron_expression, interval_seconds, run_at, next_run_at, ",
    "start_at, end_at, org_id, owner_user_id, creator_type, creator_id, agent_id, ",
    "description, action, payload, behavior, max_runs, total_runs, last_error, ",
    "lease_owner, lease_until, is_enabled, last_run_at, created_at, updated_at, created_by, updated_by"
);

/// 管理端列表与统计共用同一套 WHERE，避免「分页条数」与「总数」口径漂移。
fn append_filters<'a>(
    builder: &mut sqlx::QueryBuilder<'a, sqlx::Sqlite>,
    query: &'a CronTriggerQuery,
) {
    if let Some(trigger_type) = query.trigger_type {
        builder
            .push(" AND trigger_type = ")
            .push_bind(trigger_type.to_i32());
    }
    if !query.behaviors.is_empty() {
        builder.push(" AND behavior IN (");
        let mut separated = builder.separated(", ");
        for behavior in &query.behaviors {
            separated.push_bind(behavior.to_i32());
        }
        separated.push_unseparated(")");
    } else if let Some(behavior) = query.behavior {
        builder
            .push(" AND behavior = ")
            .push_bind(behavior.to_i32());
    }
    if let Some(org_id) = &query.org_id {
        builder.push(" AND org_id = ").push_bind(org_id);
    }
    if let Some(owner_user_id) = &query.owner_user_id {
        builder
            .push(" AND owner_user_id = ")
            .push_bind(owner_user_id);
    }
    if !query.statuses.is_empty() {
        builder.push(" AND is_enabled IN (");
        let mut separated = builder.separated(", ");
        for status in &query.statuses {
            separated.push_bind(status.to_i32());
        }
        separated.push_unseparated(")");
    } else if let Some(is_enabled) = query.is_enabled {
        builder
            .push(" AND is_enabled = ")
            .push_bind(if is_enabled { 1 } else { 0 });
    }
    if let Some(next_run_before) = query.next_run_before {
        builder
            .push(" AND next_run_at <= ")
            .push_bind(next_run_before);
    }
    if !query.exclude_ids.is_empty() {
        builder.push(" AND id NOT IN (");
        let mut separated = builder.separated(", ");
        for id in &query.exclude_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
    }
}

#[async_trait::async_trait]
impl CronTriggerDao for CronTriggerDaoSqliteImpl {
    async fn create(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        let pool: &SqlitePool = ctx.db_pool();
        sqlx::query(
            r#"
INSERT INTO cron_triggers (
    id, name, trigger_type, cron_expression, interval_seconds, run_at, next_run_at,
    start_at, end_at, org_id, owner_user_id, creator_type, creator_id, agent_id,
    description, action, payload, behavior, max_runs, total_runs, last_error,
    lease_owner, lease_until, is_enabled, last_run_at, created_at, updated_at, created_by, updated_by
) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
"#,
        )
        .bind(&trigger.id)
        .bind(&trigger.name)
        .bind(trigger.trigger_type.to_i32())
        .bind(&trigger.cron_expression)
        .bind(trigger.interval_seconds)
        .bind(trigger.run_at)
        .bind(trigger.next_run_at)
        .bind(trigger.start_at)
        .bind(trigger.end_at)
        .bind(&trigger.org_id)
        .bind(&trigger.owner_user_id)
        .bind(trigger.creator_type)
        .bind(&trigger.creator_id)
        .bind(&trigger.agent_id)
        .bind(&trigger.description)
        .bind(&trigger.action)
        .bind(&trigger.payload)
        .bind(trigger.behavior.to_i32())
        .bind(trigger.max_runs)
        .bind(trigger.total_runs)
        .bind(&trigger.last_error)
        .bind(&trigger.lease_owner)
        .bind(trigger.lease_until)
        .bind(trigger.is_enabled)
        .bind(trigger.last_run_at)
        .bind(trigger.created_at)
        .bind(trigger.updated_at)
        .bind(&trigger.created_by)
        .bind(&trigger.updated_by)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>> {
        let pool = ctx.db_pool();
        let sql = format!("SELECT {TRIGGER_COLUMNS} FROM cron_triggers WHERE id = ?");
        let trigger = sqlx::query_as::<_, CronTriggerPo>(&sql)
            .bind(id)
            .fetch_optional(pool)
            .await?;
        Ok(trigger)
    }

    async fn list(
        &self,
        ctx: RequestContext,
        query: CronTriggerQuery,
    ) -> Result<Vec<CronTriggerPo>> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new(format!(
            "SELECT {TRIGGER_COLUMNS} FROM cron_triggers WHERE 1=1"
        ));
        append_filters(&mut builder, &query);

        if query.order_by_due {
            // 调度兜底刷新优先补最早到期的条目，避免窗口内超 limit 时漏掉紧急项
            builder.push(" ORDER BY next_run_at ASC");
        } else {
            builder.push(" ORDER BY created_at DESC");
        }
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        } else if query.offset.is_some_and(|offset| offset > 0) {
            // SQLite 的 OFFSET 必须跟在 LIMIT 后；无 limit 时用 -1 表示不限条数。
            builder.push(" LIMIT -1");
        }
        if let Some(offset) = query.offset
            && offset > 0
        {
            builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let rows = builder.build_query_as().fetch_all(pool).await?;
        Ok(rows)
    }

    async fn count(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<usize> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM cron_triggers WHERE 1=1");
        append_filters(&mut builder, &query);
        let (count,): (i64,) = builder.build_query_as().fetch_one(pool).await?;
        Ok(count as usize)
    }

    async fn update(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        let pool = ctx.db_pool();
        sqlx::query(
            r#"
UPDATE cron_triggers SET
    name = ?, trigger_type = ?, cron_expression = ?, interval_seconds = ?, run_at = ?,
    next_run_at = ?, start_at = ?, end_at = ?, org_id = ?, owner_user_id = ?,
    creator_type = ?, creator_id = ?, agent_id = ?, description = ?,
    action = ?, payload = ?, behavior = ?, max_runs = ?, is_enabled = ?,
    updated_at = ?, updated_by = ?
WHERE id = ?
"#,
        )
        .bind(&trigger.name)
        .bind(trigger.trigger_type.to_i32())
        .bind(&trigger.cron_expression)
        .bind(trigger.interval_seconds)
        .bind(trigger.run_at)
        .bind(trigger.next_run_at)
        .bind(trigger.start_at)
        .bind(trigger.end_at)
        .bind(&trigger.org_id)
        .bind(&trigger.owner_user_id)
        .bind(trigger.creator_type)
        .bind(&trigger.creator_id)
        .bind(&trigger.agent_id)
        .bind(&trigger.description)
        .bind(&trigger.action)
        .bind(&trigger.payload)
        .bind(trigger.behavior.to_i32())
        .bind(trigger.max_runs)
        .bind(trigger.is_enabled)
        .bind(trigger.updated_at)
        .bind(&trigger.updated_by)
        .bind(&trigger.id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp_ms();
        let updated_by = ctx.caller_id_or_system();
        sqlx::query(
            r#"
UPDATE cron_triggers SET is_enabled = 0, updated_at = ?, updated_by = ? WHERE id = ?
"#,
        )
        .bind(now)
        .bind(updated_by)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn claim_due(
        &self,
        ctx: RequestContext,
        now: i64,
        lease_owner: &str,
        lease_until: i64,
        limit: i32,
    ) -> Result<Vec<CronTriggerPo>> {
        let pool: &SqlitePool = ctx.db_pool();
        let mut tx = pool.begin().await?;

        // 1) 候选：到期、启用、生效窗口内、租约空闲
        let select_sql = format!(
            "SELECT {TRIGGER_COLUMNS} FROM cron_triggers \
             WHERE next_run_at <= ? AND is_enabled = 1 \
             AND (start_at IS NULL OR start_at <= ?) \
             AND (end_at IS NULL OR end_at >= ?) \
             AND (lease_until IS NULL OR lease_until <= ?) \
             ORDER BY next_run_at ASC LIMIT ?"
        );
        let candidates = sqlx::query_as::<_, CronTriggerPo>(&select_sql)
            .bind(now)
            .bind(now)
            .bind(now)
            .bind(now)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?;

        // 2) 逐行条件领取；SQLite 单写者 + 事务保证 check-and-set 原子性
        let mut claimed = Vec::with_capacity(candidates.len());
        for trigger in candidates {
            let result = sqlx::query(
                r#"UPDATE cron_triggers SET lease_owner = ?, lease_until = ?, updated_at = ?
                   WHERE id = ? AND (lease_until IS NULL OR lease_until <= ?)"#,
            )
            .bind(lease_owner)
            .bind(lease_until)
            .bind(now)
            .bind(&trigger.id)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            if result.rows_affected() == 1 {
                let mut claimed_trigger = trigger;
                claimed_trigger.lease_owner = Some(lease_owner.to_string());
                claimed_trigger.lease_until = Some(lease_until);
                claimed.push(claimed_trigger);
            }
        }

        tx.commit().await?;
        Ok(claimed)
    }

    async fn claim_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
        now: i64,
        lease_owner: &str,
        lease_until: i64,
    ) -> Result<Option<CronTriggerPo>> {
        let pool: &SqlitePool = ctx.db_pool();
        let mut tx = pool.begin().await?;

        // 候选只针对指定 id：到期 / 启用 / 生效窗口内 / 租约空闲。
        let select_sql = format!(
            "SELECT {TRIGGER_COLUMNS} FROM cron_triggers \
             WHERE id = ? AND next_run_at <= ? AND is_enabled = 1 \
             AND (start_at IS NULL OR start_at <= ?) \
             AND (end_at IS NULL OR end_at >= ?) \
             AND (lease_until IS NULL OR lease_until <= ?)"
        );
        let Some(trigger) = sqlx::query_as::<_, CronTriggerPo>(&select_sql)
            .bind(id)
            .bind(now)
            .bind(now)
            .bind(now)
            .bind(now)
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(None);
        };

        // check-and-set：租约条件与查询一致，SQLite 单写者 + 事务保证原子性。
        let result = sqlx::query(
            r#"UPDATE cron_triggers SET lease_owner = ?, lease_until = ?, updated_at = ?
               WHERE id = ? AND (lease_until IS NULL OR lease_until <= ?)"#,
        )
        .bind(lease_owner)
        .bind(lease_until)
        .bind(now)
        .bind(id)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() != 1 {
            return Ok(None);
        }

        tx.commit().await?;
        let mut claimed = trigger;
        claimed.lease_owner = Some(lease_owner.to_string());
        claimed.lease_until = Some(lease_until);
        Ok(Some(claimed))
    }

    async fn update_next_run_at(
        &self,
        ctx: RequestContext,
        id: &str,
        next_run_at: i64,
        last_run_at: i64,
    ) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp_ms();
        let updated_by = ctx.caller_id_or_system();
        sqlx::query(
            r#"
UPDATE cron_triggers SET next_run_at = ?, last_run_at = ?, total_runs = total_runs + 1,
    last_error = NULL, lease_owner = NULL, lease_until = NULL,
    updated_at = ?, updated_by = ?
WHERE id = ?
"#,
        )
        .bind(next_run_at)
        .bind(last_run_at)
        .bind(now)
        .bind(updated_by)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn record_failure(&self, ctx: RequestContext, id: &str, error: &str) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp_ms();
        let updated_by = ctx.caller_id_or_system();
        // 只保留最近一条错误摘要，避免 long-running 触发器把该字段撑爆
        let summary: String = error.chars().take(500).collect();
        sqlx::query(
            r#"UPDATE cron_triggers SET last_error = ?, updated_at = ?, updated_by = ? WHERE id = ?"#,
        )
        .bind(summary)
        .bind(now)
        .bind(updated_by)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn complete_run(&self, ctx: RequestContext, id: &str, executed_at: i64) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp_ms();
        let updated_by = ctx.caller_id_or_system();
        // 单条原子语句：计数在库里自增，绝不「读出来 +1 再写回」——那会与并发执行
        // 互相覆盖。同时清租约、清错误、置终态禁用。
        sqlx::query(
            r#"
UPDATE cron_triggers SET
    total_runs = total_runs + 1, is_enabled = 0, last_run_at = ?,
    last_error = NULL, lease_owner = NULL, lease_until = NULL,
    updated_at = ?, updated_by = ?
WHERE id = ?
"#,
        )
        .bind(executed_at)
        .bind(now)
        .bind(updated_by)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }
}
