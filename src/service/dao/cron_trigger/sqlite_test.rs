//! Cron Trigger DAO SQLite 单元测试

use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::service::dao::cron_trigger::{self, CronTriggerDao, CronTriggerQuery};
use common::enums::TriggerType;
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

fn init_test_env(pool: SqlitePool) -> (Arc<dyn CronTriggerDao + Send + Sync>, RequestContext) {
    let dao = cron_trigger::new();
    let ctx = new_ctx("test-user", pool);
    (dao, ctx)
}

fn create_test_trigger(
    id: &str,
    name: &str,
    trigger_type: TriggerType,
    next_run_at: i64,
) -> CronTriggerPo {
    CronTriggerPo::new(
        id.to_string(),
        name.to_string(),
        trigger_type,
        next_run_at,
        Some("test-user".to_string()),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn test_create_and_get_by_id(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let trigger = create_test_trigger("trigger-1", "test-cron", TriggerType::Cron, 1000);

    dao.create(ctx.clone(), &trigger).await?;

    let found = dao.get_by_id(ctx.clone(), &trigger.id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, trigger.id);
    assert_eq!(found.name, "test-cron");
    assert_eq!(found.trigger_type, TriggerType::Cron);
    assert_eq!(found.next_run_at, 1000);
    assert_eq!(found.is_enabled, 1);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_trigger_type_and_is_enabled(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let cron_trigger = create_test_trigger("trigger-1", "cron-job", TriggerType::Cron, 1000);
    let interval_trigger =
        create_test_trigger("trigger-2", "interval-job", TriggerType::Interval, 2000);
    let once_trigger = create_test_trigger("trigger-3", "once-job", TriggerType::Once, 3000);

    dao.create(ctx.clone(), &cron_trigger).await?;
    dao.create(ctx.clone(), &interval_trigger).await?;
    dao.create(ctx.clone(), &once_trigger).await?;
    dao.delete(ctx.clone(), &once_trigger.id).await?;

    let all_triggers = dao.list(ctx.clone(), CronTriggerQuery::default()).await?;
    assert_eq!(all_triggers.len(), 3);

    let cron_only = dao
        .list(
            ctx.clone(),
            CronTriggerQuery {
                trigger_type: Some(TriggerType::Cron),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(cron_only.len(), 1);
    assert_eq!(cron_only[0].id, "trigger-1");

    let enabled_only = dao
        .list(
            ctx.clone(),
            CronTriggerQuery {
                is_enabled: Some(true),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(enabled_only.len(), 2);

    let disabled_only = dao
        .list(
            ctx.clone(),
            CronTriggerQuery {
                is_enabled: Some(false),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(disabled_only.len(), 1);
    assert_eq!(disabled_only[0].id, "trigger-3");

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_with_limit(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let trigger1 = create_test_trigger("trigger-1", "job-1", TriggerType::Cron, 1000);
    let trigger2 = create_test_trigger("trigger-2", "job-2", TriggerType::Cron, 2000);
    let trigger3 = create_test_trigger("trigger-3", "job-3", TriggerType::Cron, 3000);

    dao.create(ctx.clone(), &trigger1).await?;
    dao.create(ctx.clone(), &trigger2).await?;
    dao.create(ctx.clone(), &trigger3).await?;

    let limited = dao
        .list(
            ctx.clone(),
            CronTriggerQuery {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(limited.len(), 2);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let mut trigger = create_test_trigger("trigger-1", "old-name", TriggerType::Cron, 1000);
    dao.create(ctx.clone(), &trigger).await?;

    trigger.name = "new-name".to_string();
    trigger.trigger_type = TriggerType::Interval;
    trigger.next_run_at = 2000;
    trigger.touch(Some("modifier".to_string()));
    dao.update(ctx.clone(), &trigger).await?;

    let found = dao.get_by_id(ctx.clone(), &trigger.id).await?.unwrap();
    assert_eq!(found.name, "new-name");
    assert_eq!(found.trigger_type, TriggerType::Interval);
    assert_eq!(found.next_run_at, 2000);
    assert_eq!(found.updated_by.as_deref(), Some("modifier"));

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_delete_sets_is_enabled_to_zero(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let trigger = create_test_trigger("trigger-1", "test-cron", TriggerType::Cron, 1000);
    dao.create(ctx.clone(), &trigger).await?;

    dao.delete(ctx.clone(), &trigger.id).await?;

    let found = dao.get_by_id(ctx.clone(), &trigger.id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.is_enabled, 0);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_due(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let due_trigger1 = create_test_trigger("trigger-1", "due-1", TriggerType::Cron, 1000);
    let due_trigger2 = create_test_trigger("trigger-2", "due-2", TriggerType::Interval, 2000);
    let future_trigger = create_test_trigger("trigger-3", "future", TriggerType::Once, 5000);
    let disabled_trigger = create_test_trigger("trigger-4", "disabled", TriggerType::Cron, 1500);

    dao.create(ctx.clone(), &due_trigger1).await?;
    dao.create(ctx.clone(), &due_trigger2).await?;
    dao.create(ctx.clone(), &future_trigger).await?;
    dao.create(ctx.clone(), &disabled_trigger).await?;
    dao.delete(ctx.clone(), &disabled_trigger.id).await?;

    let due = dao.list_due(ctx.clone(), 3000, 10).await?;
    assert_eq!(due.len(), 2);
    assert_eq!(due[0].id, "trigger-1");
    assert_eq!(due[1].id, "trigger-2");

    let limited_due = dao.list_due(ctx.clone(), 3000, 1).await?;
    assert_eq!(limited_due.len(), 1);
    assert_eq!(limited_due[0].id, "trigger-1");

    let no_due = dao.list_due(ctx.clone(), 500, 10).await?;
    assert!(no_due.is_empty());

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update_next_run_at(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let trigger = create_test_trigger("trigger-1", "test-cron", TriggerType::Cron, 1000);
    dao.create(ctx.clone(), &trigger).await?;

    dao.update_next_run_at(ctx.clone(), &trigger.id, 2000, 1500)
        .await?;

    let found = dao.get_by_id(ctx.clone(), &trigger.id).await?.unwrap();
    assert_eq!(found.next_run_at, 2000);
    assert_eq!(found.last_run_at, Some(1500));
    assert_eq!(found.updated_by.as_deref(), Some("test-user"));

    Ok(())
}
