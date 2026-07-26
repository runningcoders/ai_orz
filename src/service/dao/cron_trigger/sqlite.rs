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

#[async_trait::async_trait]
impl CronTriggerDao for CronTriggerDaoSqliteImpl {
    async fn create(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        let pool: &SqlitePool = ctx.db_pool();
        let trigger_type = trigger.trigger_type.to_i32();
        sqlx::query(
            r#"
INSERT INTO cron_triggers (id, name, trigger_type, cron_expression, interval_seconds, run_at, next_run_at, is_enabled, payload, last_run_at, created_at, updated_at, created_by, updated_by) VALUES (
?,
?,
?,
?,
?,
?,
?,
?,
?,
?,
?,
?,
?,
?
)
"#,
        )
        .bind(&trigger.id)
        .bind(&trigger.name)
        .bind(trigger_type)
        .bind(&trigger.cron_expression)
        .bind(trigger.interval_seconds)
        .bind(trigger.run_at)
        .bind(trigger.next_run_at)
        .bind(trigger.is_enabled)
        .bind(&trigger.payload)
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
        let trigger = sqlx::query_as::<_, CronTriggerPo>(
            r#"
SELECT id, name, trigger_type, cron_expression, interval_seconds, run_at, next_run_at, is_enabled, payload, last_run_at, created_at, updated_at, created_by, updated_by
FROM cron_triggers
WHERE id = ?
"#,
        )
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
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, trigger_type, cron_expression, interval_seconds, run_at, next_run_at, is_enabled, payload, last_run_at, created_at, updated_at, created_by, updated_by FROM cron_triggers WHERE 1=1"#,
        );

        if let Some(trigger_type) = query.trigger_type {
            builder
                .push(" AND trigger_type = ")
                .push_bind(trigger_type.to_i32());
        }
        if let Some(is_enabled) = query.is_enabled {
            builder
                .push(" AND is_enabled = ")
                .push_bind(if is_enabled { 1 } else { 0 });
        }

        builder.push(" ORDER BY created_at DESC");
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        let rows = builder.build_query_as().fetch_all(pool).await?;
        Ok(rows)
    }

    async fn update(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()> {
        let pool = ctx.db_pool();
        let trigger_type = trigger.trigger_type.to_i32();
        sqlx::query(
            r#"
UPDATE cron_triggers SET name = ?, trigger_type = ?, cron_expression = ?, interval_seconds = ?, run_at = ?, next_run_at = ?, is_enabled = ?, payload = ?, last_run_at = ?, updated_at = ?, updated_by = ? WHERE id = ?
"#,
        )
        .bind(&trigger.name)
        .bind(trigger_type)
        .bind(&trigger.cron_expression)
        .bind(trigger.interval_seconds)
        .bind(trigger.run_at)
        .bind(trigger.next_run_at)
        .bind(trigger.is_enabled)
        .bind(&trigger.payload)
        .bind(trigger.last_run_at)
        .bind(trigger.updated_at)
        .bind(&trigger.updated_by)
        .bind(&trigger.id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp();
        let updated_by = ctx.uid();
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

    async fn list_due(
        &self,
        ctx: RequestContext,
        now: i64,
        limit: i32,
    ) -> Result<Vec<CronTriggerPo>> {
        let pool = ctx.db_pool();
        let triggers = sqlx::query_as::<_, CronTriggerPo>(
            r#"
SELECT id, name, trigger_type, cron_expression, interval_seconds, run_at, next_run_at, is_enabled, payload, last_run_at, created_at, updated_at, created_by, updated_by
FROM cron_triggers
WHERE next_run_at <= ? AND is_enabled = 1
ORDER BY next_run_at ASC
LIMIT ?
"#,
        )
        .bind(now)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(triggers)
    }

    async fn update_next_run_at(
        &self,
        ctx: RequestContext,
        id: &str,
        next_run_at: i64,
        last_run_at: i64,
    ) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp();
        let updated_by = ctx.uid();
        sqlx::query(
            r#"
UPDATE cron_triggers SET next_run_at = ?, last_run_at = ?, updated_at = ?, updated_by = ? WHERE id = ?
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
}
