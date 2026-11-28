//! Cron Trigger DAO SQLite 单元测试

use crate::models::cron_trigger::CronTriggerPo;
use crate::pkg::RequestContext;
use crate::service::dao::cron_trigger::{self, CronTriggerDao, CronTriggerQuery};
use common::enums::{TriggerBehavior, TriggerStatus, TriggerType};
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

/// 通用 update 只写配置列：管理端拿着旧快照「读 → 改 → 写」时，
/// 不能把执行侧的累计次数回退、也不能清掉 `last_run_at` / 租约。
#[sqlx::test(migrations = "./migrations")]
async fn test_update_keeps_runtime_columns(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let mut trigger = create_test_trigger("trigger-1", "s", TriggerType::Cron, 1000);
    dao.create(ctx.clone(), &trigger).await?;

    // 模拟执行侧：领取 → 成功收尾（total_runs=1, last_run_at=1500, 租约已清）
    dao.claim_due(ctx.clone(), 1000, "worker-1", 9000, 10)
        .await?;
    dao.update_next_run_at(ctx.clone(), &trigger.id, 2000, 1500)
        .await?;

    // 管理端用创建时的旧快照改名，并试图写回运行态（这些都是陈旧值）
    trigger.name = "改名".into();
    trigger.total_runs = 0;
    trigger.last_run_at = None;
    trigger.lease_owner = Some("stale".into());
    trigger.lease_until = Some(1);
    dao.update(ctx.clone(), &trigger).await?;

    let after = dao.get_by_id(ctx.clone(), &trigger.id).await?.unwrap();
    assert_eq!(after.name, "改名");
    assert_eq!(after.total_runs, 1, "累计次数不得被配置更新回退");
    assert_eq!(after.last_run_at, Some(1500), "上次执行时刻不得被清空");
    assert_eq!(after.lease_owner, None, "租约不得被陈旧快照覆盖");
    assert_eq!(after.lease_until, None);

    Ok(())
}

/// 终态收尾必须一条语句完成「自增 + 禁用 + 清租约 + 记最后执行时刻」。
#[sqlx::test(migrations = "./migrations")]
async fn test_complete_run(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let trigger = create_test_trigger("trigger-1", "once", TriggerType::Once, 1000);
    dao.create(ctx.clone(), &trigger).await?;
    dao.claim_due(ctx.clone(), 1000, "worker-1", 9000, 10)
        .await?;

    dao.complete_run(ctx.clone(), &trigger.id, 1234).await?;

    let found = dao.get_by_id(ctx.clone(), &trigger.id).await?.unwrap();
    assert_eq!(found.total_runs, 1);
    assert_eq!(found.is_enabled, 0);
    assert_eq!(found.last_run_at, Some(1234));
    assert_eq!(found.lease_owner, None);
    assert_eq!(found.lease_until, None);
    assert_eq!(found.last_error, None);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_extended_fields_roundtrip(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let mut trigger = create_test_trigger("trigger-1", "schedule", TriggerType::Schedule, 1000);
    trigger.org_id = Some("org-1".to_string());
    trigger.owner_user_id = Some("user-1".to_string());
    trigger.creator_type = Some(1);
    trigger.creator_id = Some("agent-1".to_string());
    trigger.agent_id = Some("agent-2".to_string());
    trigger.description = "每天 9 点汇总".to_string();
    trigger.action = Some("agent_schedule".to_string());
    trigger.behavior = TriggerBehavior::AsyncAgentWakeup;
    trigger.start_at = Some(500);
    trigger.end_at = Some(5000);
    trigger.max_runs = Some(3);

    dao.create(ctx.clone(), &trigger).await?;
    let found = dao.get_by_id(ctx.clone(), &trigger.id).await?.unwrap();
    assert_eq!(found.org_id.as_deref(), Some("org-1"));
    assert_eq!(found.owner_user_id.as_deref(), Some("user-1"));
    assert_eq!(found.creator_type, Some(1));
    assert_eq!(found.agent_id.as_deref(), Some("agent-2"));
    assert_eq!(found.description, "每天 9 点汇总");
    assert_eq!(found.resolved_action().as_deref(), Some("agent_schedule"));
    assert_eq!(found.behavior, TriggerBehavior::AsyncAgentWakeup);
    assert_eq!(found.start_at, Some(500));
    assert_eq!(found.max_runs, Some(3));
    assert_eq!(found.total_runs, 0);

    // 按 behavior / owner 过滤
    let filtered = dao
        .list(
            ctx.clone(),
            CronTriggerQuery {
                behavior: Some(TriggerBehavior::AsyncAgentWakeup),
                owner_user_id: Some("user-1".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "trigger-1");

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_claim_due_lease(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let t1 = create_test_trigger("trigger-1", "due-1", TriggerType::Cron, 1000);
    let t2 = create_test_trigger("trigger-2", "due-2", TriggerType::Cron, 2000);
    let future = create_test_trigger("trigger-3", "future", TriggerType::Cron, 9000);
    for t in [&t1, &t2, &future] {
        dao.create(ctx.clone(), t).await?;
    }

    // 首次领取：拿到 2 条到期记录
    let claimed = dao
        .claim_due(ctx.clone(), 3000, "instance-a", 8000, 10)
        .await?;
    assert_eq!(claimed.len(), 2);
    assert_eq!(claimed[0].id, "trigger-1");
    assert_eq!(claimed[1].id, "trigger-2");

    // 租约未到期：另一实例领取不到
    let none = dao
        .claim_due(ctx.clone(), 3000, "instance-b", 8000, 10)
        .await?;
    assert!(none.is_empty());

    // 租约到期后：可重新领取
    let reclaimed = dao
        .claim_due(ctx.clone(), 8000, "instance-b", 12000, 10)
        .await?;
    assert_eq!(reclaimed.len(), 2);

    // 窗口外的到期记录不会被领取
    let mut outside = create_test_trigger("trigger-4", "outside", TriggerType::Cron, 1000);
    outside.start_at = Some(9000);
    dao.create(ctx.clone(), &outside).await?;
    let still = dao
        .claim_due(ctx.clone(), 8000, "instance-c", 12000, 10)
        .await?;
    assert!(still.iter().all(|t| t.id != "trigger-4"));

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_claim_by_id_is_exclusive_and_scoped(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let due = create_test_trigger("trigger-1", "due", TriggerType::Cron, 1000);
    let future = create_test_trigger("trigger-2", "future", TriggerType::Cron, 9000);
    for t in [&due, &future] {
        dao.create(ctx.clone(), t).await?;
    }

    // 按 id 只领指定条，不会取到另一条到期的（用独立的 due 对照）
    let another_due = create_test_trigger("trigger-3", "another", TriggerType::Cron, 500);
    dao.create(ctx.clone(), &another_due).await?;
    let claimed = dao
        .claim_by_id(ctx.clone(), "trigger-1", 3000, "instance-a", 8000)
        .await?;
    assert_eq!(claimed.map(|t| t.id), Some("trigger-1".to_string()));

    // 租约期内另一实例领不到同一条
    let none = dao
        .claim_by_id(ctx.clone(), "trigger-1", 3000, "instance-b", 8000)
        .await?;
    assert!(none.is_none());

    // 未到期的条目领不到
    let not_due = dao
        .claim_by_id(ctx.clone(), "trigger-2", 3000, "instance-a", 8000)
        .await?;
    assert!(not_due.is_none());

    // 租约到期后可重新领取
    let reclaimed = dao
        .claim_by_id(ctx.clone(), "trigger-1", 8000, "instance-b", 12000)
        .await?;
    assert_eq!(reclaimed.map(|t| t.id), Some("trigger-1".to_string()));

    // 不存在的 id 返回 None
    let missing = dao
        .claim_by_id(ctx.clone(), "no-such", 8000, "instance-b", 12000)
        .await?;
    assert!(missing.is_none());

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update_next_run_at_clears_lease_and_counts(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    let trigger = create_test_trigger("trigger-1", "cron", TriggerType::Cron, 1000);
    dao.create(ctx.clone(), &trigger).await?;
    dao.claim_due(ctx.clone(), 1500, "instance-a", 9000, 10)
        .await?;

    dao.update_next_run_at(ctx.clone(), &trigger.id, 2500, 1500)
        .await?;
    let found = dao.get_by_id(ctx.clone(), &trigger.id).await?.unwrap();
    assert_eq!(found.next_run_at, 2500);
    assert_eq!(found.total_runs, 1);
    assert_eq!(found.lease_owner, None);
    assert_eq!(found.lease_until, None);

    Ok(())
}

/// 管理端「状态列表 + 分页」必须在 SQL 层完成：总数不含分页，第二页不得混入取消项。
#[sqlx::test(migrations = "./migrations")]
async fn test_list_statuses_with_pagination_and_count(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    for i in 1..=4 {
        dao.create(
            ctx.clone(),
            &create_test_trigger(
                &format!("trigger-{i}"),
                &format!("job-{i}"),
                TriggerType::Cron,
                i * 1000,
            ),
        )
        .await?;
    }
    dao.delete(ctx.clone(), "trigger-4").await?;

    let enabled = CronTriggerQuery {
        statuses: vec![TriggerStatus::Enabled],
        ..Default::default()
    };
    assert_eq!(dao.count(ctx.clone(), enabled.clone()).await?, 3);

    let page2 = CronTriggerQuery {
        offset: Some(2),
        limit: Some(2),
        ..enabled.clone()
    };
    let rows = dao.list(ctx.clone(), page2).await?;
    assert_eq!(rows.len(), 1);
    // created_at 同毫秒时行序不稳定，只断言分页确实取到启用项且不含取消项。
    assert_eq!(rows[0].is_enabled, 1);

    let all = CronTriggerQuery {
        statuses: vec![TriggerStatus::Enabled, TriggerStatus::Disabled],
        ..Default::default()
    };
    assert_eq!(dao.count(ctx.clone(), all.clone()).await?, 4);
    assert_eq!(dao.list(ctx.clone(), all).await?.len(), 4);

    Ok(())
}

/// DB 兜底刷新窗口：时间上界 + 已在派发队列的 id 排除，都在 SQL 层完成，
/// 不让「已经在队列里」的记录再占查询/传输带宽。
#[sqlx::test(migrations = "./migrations")]
async fn test_list_dispatch_window_excludes_queued_ids(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    for (id, next_run_at) in [("queued", 1000), ("fresh", 2000), ("far", 9000)] {
        dao.create(
            ctx.clone(),
            &create_test_trigger(id, id, TriggerType::Cron, next_run_at),
        )
        .await?;
    }

    let rows = dao
        .list(
            ctx.clone(),
            CronTriggerQuery {
                next_run_before: Some(5000),
                exclude_ids: vec!["queued".to_string()],
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "fresh");

    Ok(())
}

/// 兜底刷新按 next_run_at ASC：窗口内超 limit 时先补最早到期的条目，
/// 而不是按 created_at DESC 漏掉最紧急的那批。
#[sqlx::test(migrations = "./migrations")]
async fn test_list_order_by_due_prioritizes_earliest(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);
    for (id, next_run_at) in [("late", 4000), ("early", 1000), ("mid", 2000)] {
        dao.create(
            ctx.clone(),
            &create_test_trigger(id, id, TriggerType::Cron, next_run_at),
        )
        .await?;
    }

    let rows = dao
        .list(
            ctx.clone(),
            CronTriggerQuery {
                next_run_before: Some(5000),
                order_by_due: true,
                limit: Some(2),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(
        rows.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["early", "mid"]
    );

    Ok(())
}
