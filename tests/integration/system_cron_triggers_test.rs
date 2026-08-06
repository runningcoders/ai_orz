//! Integration test for system-level default cron triggers
//! (`ensure_system_cron_triggers`).
//!
//! Verifies:
//! - Both `agent_rest` and `project_followup` triggers are created after init
//! - Idempotency: calling `ensure_system_cron_triggers` again doesn't create
//!   duplicates
//! - If a user already has an `agent_rest` trigger, the system doesn't create
//!   another one
//!
//! Note: `init_full_test_env` (called once per process via `OnceCell`) invokes
//! `consumer::init()`, which in turn calls `ensure_system_cron_triggers`. So
//! by the time each test runs, the two default system triggers already exist
//! in the shared global DB.
//!
//! Parallel-test safety: integration tests share a single global DB and run in
//! parallel, so counting "all triggers with action X" would be flaky (another
//! test could add one concurrently). Instead, we identify the system-default
//! triggers by their unique names ("系统默认-Agent 睡眠沉淀" /
//! "系统默认-项目进度巡检") which are stable across runs.

#[path = "../common/mod.rs"]
mod common;

use ::common::enums::TriggerType;
use ai_orz::models::cron_trigger::CronTriggerPo;
use ai_orz::pkg::RequestContext;
use ai_orz::service::dao::cron_trigger::CronTriggerQuery;
use ai_orz::service::domain::system::{self, domain};
use sqlx::SqlitePool;

/// System-default agent_rest trigger name (must match `ensure_system_cron_triggers`).
const SYSTEM_AGENT_REST_NAME: &str = "系统默认-Agent 睡眠沉淀";
/// System-default project_followup trigger name (must match `ensure_system_cron_triggers`).
const SYSTEM_PROJECT_FOLLOWUP_NAME: &str = "系统默认-项目进度巡检";

/// Fetch all triggers from the DB.
async fn list_all_triggers(ctx: &RequestContext) -> Vec<CronTriggerPo> {
    domain()
        .cron_manager()
        .list_triggers(ctx.clone(), CronTriggerQuery::default())
        .await
        .expect("list_triggers should succeed")
}

/// Count triggers with the given exact name.
async fn count_triggers_named(ctx: &RequestContext, name: &str) -> usize {
    list_all_triggers(ctx)
        .await
        .into_iter()
        .filter(|t| t.name == name)
        .count()
}

/// Find the first trigger with the given exact name.
async fn find_trigger_named(ctx: &RequestContext, name: &str) -> Option<CronTriggerPo> {
    list_all_triggers(ctx)
        .await
        .into_iter()
        .find(|t| t.name == name)
}

/// Both system-default triggers (`agent_rest` + `project_followup`) exist
/// after the test environment is initialized.
#[sqlx::test]
async fn test_system_cron_triggers_created(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool).await;

    // agent_rest trigger should exist (created by consumer::init → ensure_system_cron_triggers)
    let agent_rest = find_trigger_named(&ctx, SYSTEM_AGENT_REST_NAME)
        .await
        .expect("system-default agent_rest trigger should exist after init");
    assert_eq!(agent_rest.trigger_type, TriggerType::Interval);
    assert_eq!(agent_rest.interval_seconds, Some(4 * 3600));
    assert_eq!(agent_rest.is_enabled, 1);
    assert!(
        agent_rest.payload.contains("\"agent_rest\""),
        "agent_rest payload should contain action=agent_rest, got: {}",
        agent_rest.payload
    );
    assert!(
        agent_rest.payload.contains("\"settle_limit\":10"),
        "agent_rest payload should contain settle_limit=10, got: {}",
        agent_rest.payload
    );

    // project_followup trigger should exist
    let project_followup = find_trigger_named(&ctx, SYSTEM_PROJECT_FOLLOWUP_NAME)
        .await
        .expect("system-default project_followup trigger should exist after init");
    assert_eq!(project_followup.trigger_type, TriggerType::Interval);
    assert_eq!(project_followup.interval_seconds, Some(3600));
    assert_eq!(project_followup.is_enabled, 1);
    assert!(
        project_followup.payload.contains("\"project_followup\""),
        "project_followup payload should contain action=project_followup, got: {}",
        project_followup.payload
    );
}

/// Idempotency: calling `ensure_system_cron_triggers` again does not create
/// additional system-default triggers.
///
/// Uses the unique system-default trigger names to filter out user-created
/// triggers that may be added by other parallel tests in the same suite.
#[sqlx::test]
async fn test_system_cron_triggers_idempotent(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool).await;

    let before_agent_rest = count_triggers_named(&ctx, SYSTEM_AGENT_REST_NAME).await;
    let before_project_followup = count_triggers_named(&ctx, SYSTEM_PROJECT_FOLLOWUP_NAME).await;
    assert_eq!(
        before_agent_rest, 1,
        "baseline should have exactly 1 system agent_rest trigger"
    );
    assert_eq!(
        before_project_followup, 1,
        "baseline should have exactly 1 system project_followup trigger"
    );

    // Call ensure_system_cron_triggers again — should be a no-op since both
    // default triggers already exist.
    system::ensure_system_cron_triggers(&ctx)
        .await
        .expect("ensure_system_cron_triggers should succeed on repeat call");

    let after_agent_rest = count_triggers_named(&ctx, SYSTEM_AGENT_REST_NAME).await;
    let after_project_followup = count_triggers_named(&ctx, SYSTEM_PROJECT_FOLLOWUP_NAME).await;

    assert_eq!(
        before_agent_rest, after_agent_rest,
        "system-default agent_rest trigger count should not change after re-calling ensure_system_cron_triggers"
    );
    assert_eq!(
        before_project_followup, after_project_followup,
        "system-default project_followup trigger count should not change after re-calling ensure_system_cron_triggers"
    );
}

/// If a user (or any other source) has already created an `agent_rest`
/// trigger, calling `ensure_system_cron_triggers` must not add another one.
///
/// Setup: the system-default `agent_rest` already exists from `consumer::init`.
/// We then manually add a *second* user-defined `agent_rest` trigger (different
/// ID/name) and call `ensure_system_cron_triggers`. The system should detect
/// that `agent_rest` already exists (via payload substring match) and skip
/// creation.
#[sqlx::test]
async fn test_system_cron_triggers_no_duplicate_when_user_has_agent_rest(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool).await;

    // Snapshot the system-default trigger count before any user action.
    let before_system_agent_rest = count_triggers_named(&ctx, SYSTEM_AGENT_REST_NAME).await;
    assert_eq!(
        before_system_agent_rest, 1,
        "baseline should have exactly 1 system-default agent_rest trigger"
    );

    // Manually create a user-defined agent_rest trigger (simulating a user
    // adding their own custom agent_rest before system init ran).
    let user_trigger_name = format!("User-defined agent_rest-{}", uuid::Uuid::now_v7());
    let mut user_trigger = CronTriggerPo::new(
        uuid::Uuid::now_v7().to_string(),
        user_trigger_name.clone(),
        TriggerType::Interval,
        ::common::constants::utils::current_timestamp_ms() + 4 * 3600_000,
        Some("test-user".into()),
    );
    user_trigger.interval_seconds = Some(4 * 3600);
    user_trigger.payload =
        r#"{"action":"agent_rest","extra":{"agent_id":"user-agent-001","settle_limit":5}}"#.into();
    user_trigger.is_enabled = 1;
    domain()
        .cron_manager()
        .create_trigger(ctx.clone(), &user_trigger)
        .await
        .expect("user trigger create should succeed");

    // Verify the user-defined trigger was added.
    assert_eq!(
        count_triggers_named(&ctx, &user_trigger_name).await,
        1,
        "user-defined agent_rest trigger should exist after creation"
    );

    // Now call ensure_system_cron_triggers — should NOT add another
    // system-default agent_rest trigger (since one with action=agent_rest
    // already exists in the user's trigger).
    system::ensure_system_cron_triggers(&ctx)
        .await
        .expect("ensure_system_cron_triggers should succeed");

    let after_system_agent_rest = count_triggers_named(&ctx, SYSTEM_AGENT_REST_NAME).await;
    assert_eq!(
        before_system_agent_rest, after_system_agent_rest,
        "no new system-default agent_rest should be created when one already exists"
    );
}
