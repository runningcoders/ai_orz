//! Integration test for `CronTriggerConsumer` `project_followup` action.
//!
//! Verifies the Agent Loop Engine "scenario 3" (定时补偿) flow:
//! - Create a project with `owner_agent_id` set and transition it to InProgress
//! - Backdate the project's activity so it satisfies the stall judgment
//! - Ensure the Owner Agent runtime state is Idle
//! - Fire a `CronTriggerEvent` with payload `{"action":"project_followup","extra":{}}`
//! - `CronTriggerConsumer::on_event` should send a `ProjectFollowupNotification`
//!   message to the Owner Agent with `project_id` filled

#[path = "../common/mod.rs"]
mod common;

extern crate common as common_ext;

use crate::common::TestApp;
use ai_orz::consumer::scheduler::CronTriggerConsumer;
use ai_orz::pkg::RequestContext;
use ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager;
use ai_orz::pkg::aop::Consumer;
use ai_orz::service::domain::message;
use ai_orz::service::domain::message::SendToAgentCommand;
use common_ext::enums::{MessageRole, MessageType};
use serde_json::json;
use sqlx::SqlitePool;

/// 文件级串行锁。
///
/// 本文件所有用例共享同一个全局 DB（见 `common::init_full_test_env` 的 `OnceCell`
/// 单例），而 `project_followup` 消费者扫描的是**全部** InProgress 项目 ——
/// 若不串行，A 用例触发消费时会把 B 用例的项目一并跟进（或反之），断言串味。
/// 用一把锁把每个用例的「建项目 → 触发 → 断言」整体串起来，消除并发交叉。
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 当前毫秒时间戳
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 把项目的活动时间拨到 3 小时前：默认停滞阈值 2h，使其满足「停滞」判据。
///
/// 停滞判据取 `max(项目.updated_at, 各任务.updated_at)`，故项目行与任务行
/// 都要拨旧（任务写路径不更新父项目 updated_at，但任务自身的 updated_at
/// 参与 max 计算——刚流转过的任务会让项目被判为「未停滞」）。
async fn backdate_project_activity(pool: &SqlitePool, project_id: &str) {
    let three_hours_ago = now_ms() - 3 * 3600 * 1000;
    sqlx::query("UPDATE projects SET updated_at = ? WHERE id = ?")
        .bind(three_hours_ago)
        .bind(project_id)
        .execute(pool)
        .await
        .expect("backdate project failed");
    sqlx::query("UPDATE tasks SET updated_at = ? WHERE project_id = ?")
        .bind(three_hours_ago)
        .bind(project_id)
        .execute(pool)
        .await
        .expect("backdate tasks failed");
}

/// 设置项目「上次跟进时间」
async fn set_last_followup_at(pool: &SqlitePool, project_id: &str, ts: i64) {
    sqlx::query("UPDATE projects SET last_followup_at = ? WHERE id = ?")
        .bind(ts)
        .bind(project_id)
        .execute(pool)
        .await
        .expect("set last_followup_at failed");
}

/// 构造 `project_followup` 的 `CronTriggerEvent` JSON
fn build_trigger_event(trigger_id: &str, trigger_name: &str) -> serde_json::Value {
    let payload = json!({"action":"project_followup","extra":{}}).to_string();
    json!({
        "event_id": uuid::Uuid::now_v7().to_string(),
        "trigger_id": trigger_id,
        "trigger_name": trigger_name,
        "payload": payload,
        "created_at": now_ms(),
    })
}

/// Create a project via HTTP API with the given `owner_agent_id` (optional).
///
/// Returns the new project's ID.
async fn create_project_with_owner(
    app: &TestApp,
    jwt: &str,
    name: &str,
    owner_agent_id: Option<&str>,
) -> String {
    let mut req = json!({
        "name": name,
        "description": "Test project for cron project_followup",
    });
    if let Some(agent_id) = owner_agent_id {
        req["owner_agent_id"] = json!(agent_id);
    }
    let (status, body) = app.post_with_jwt("/api/v1/projects", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing id in project create response")
        .to_string()
}

/// Transition a project's status via HTTP API (PUT /projects/{id}/status).
async fn transition_project_status(app: &TestApp, jwt: &str, project_id: &str, status: &str) {
    let req = json!({
        "id": project_id,
        "status": status,
    });
    let (status_code, body) = app
        .put_with_jwt(
            &format!("/api/v1/projects/{}/status", project_id),
            &req,
            jwt,
        )
        .await;
    assert_eq!(
        status_code,
        axum::http::StatusCode::OK,
        "status transition to {} should succeed, body: {}",
        status,
        body
    );
}

/// `CronTriggerConsumer` `project_followup` action sends a
/// `ProjectFollowupNotification` message to the Owner Agent of every stalled
/// InProgress project, with `project_id` filled.
#[sqlx::test]
async fn test_cron_project_followup_sends_notification(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // Create an Agent to use as owner_agent_id
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FollowupOwner-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Create a project owned by the agent and transition it to InProgress
    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("FollowupProject-{}", uuid::Uuid::now_v7()),
        Some(&agent_id),
    )
    .await;
    transition_project_status(&app, &jwt, &project_id, "InProgress").await;

    // 新判据下项目刚创建（updated_at = now）不算停滞：把活动时间拨到 3 小时前
    backdate_project_activity(ctx.db_pool(), &project_id).await;

    // Ensure Owner Agent state is Idle so the pre-check passes
    AgentRuntimeStateManager::global().set_idle(&agent_id);

    let event_value = build_trigger_event(
        "test-trigger-project-followup",
        "test project followup trigger",
    );

    // Call CronTriggerConsumer::on_event（AOP 框架以 System ctx 调度触发器消费者）
    let consumer = CronTriggerConsumer::new();
    consumer
        .on_event(RequestContext::new_system(), event_value)
        .await
        .expect("on_event project_followup should succeed");

    // Verify the Owner Agent received a ProjectFollowupNotification message
    // with project_id filled.
    let messages = message::domain()
        .management()
        .list_by_project_id(ctx, &project_id)
        .await
        .expect("list_by_project_id should succeed");

    let followup = messages.iter().find(|m| {
        m.po.to_id == agent_id && m.po.message_type == MessageType::ProjectFollowupNotification
    });
    let followup = followup.unwrap_or_else(|| {
        panic!(
            "expected ProjectFollowupNotification for agent {} on project {}, got: {:?}",
            agent_id,
            project_id,
            messages
                .iter()
                .map(|m| (m.po.message_type, m.po.to_id.clone()))
                .collect::<Vec<_>>()
        );
    });

    assert_eq!(
        followup.po.project_id,
        Some(project_id.clone()),
        "project_id should be filled on the followup message"
    );
    assert_eq!(followup.po.to_id, agent_id);
    // 身份分层模型：巡检以项目归属用户身份中继（from_role=User），Agent 的 Final
    // 自动回复经 User 分支回到该用户；无归属用户的极端场景才落 System
    assert_eq!(followup.po.from_id, bs.user_id);
    assert_eq!(followup.po.from_role, MessageRole::User);
    assert_eq!(
        followup.po.message_type,
        MessageType::ProjectFollowupNotification
    );
    // Content should be produced by build_project_followup_content
    assert!(followup.po.content.contains("项目进度定期检查"));
    assert!(followup.po.content.contains("get_project"));
    // 消费侧预取进度汇总，正文带快照块
    assert!(followup.po.content.contains("当前进度快照"));
}

/// 回归锚点：项目内有 Busy 的 Agent（且该 Agent 就是 Owner）时，跟进通知**仍然投递**。
///
/// 旧实现有两道门禁：「项目内有 Busy Agent → 跳过」「Owner 不可用 → 跳过」。
/// 二者都把「可能有人在处理」当成「不必再通知」，结果是项目在「别人忙完却没有
/// 产生新事件」时永久停在原地 —— 而巡检正是这种情况的**唯一**兜底。
/// 现在改为照发：占线时消息停在 `Pending`，由滞留重投等它空闲。
#[sqlx::test]
async fn test_cron_project_followup_delivers_even_when_agent_busy_on_project(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FollowupBusy-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("FollowupBusyProject-{}", uuid::Uuid::now_v7()),
        Some(&agent_id),
    )
    .await;
    transition_project_status(&app, &jwt, &project_id, "InProgress").await;
    backdate_project_activity(ctx.db_pool(), &project_id).await;

    // 项目内有 Busy 的 Agent（业务上下文绑定到该项目）
    AgentRuntimeStateManager::global().set_busy(&agent_id, "m1", None, Some(&project_id));

    let consumer = CronTriggerConsumer::new();
    consumer
        .on_event(
            RequestContext::new_system(),
            build_trigger_event("test-trigger-busy", "test busy trigger"),
        )
        .await
        .expect("on_event project_followup should succeed");

    let messages = message::domain()
        .management()
        .list_by_project_id(ctx, &project_id)
        .await
        .expect("list_by_project_id should succeed");

    let followups: Vec<_> = messages
        .iter()
        .filter(|m| m.po.message_type == MessageType::ProjectFollowupNotification)
        .collect();
    assert_eq!(
        followups.len(),
        1,
        "项目内有 Busy Agent 也不应抑制巡检兜底通知（实际 {} 条）",
        followups.len()
    );

    // 全局单例跨用例复用：断言后必须复位，避免串味
    AgentRuntimeStateManager::global().set_idle(&agent_id);
}

/// ④ 距上次跟进不足 `min_followup_interval` 时跳过（节流）。
#[sqlx::test]
async fn test_cron_project_followup_skips_when_recently_followed_up(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FollowupThrottle-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("FollowupThrottleProject-{}", uuid::Uuid::now_v7()),
        Some(&agent_id),
    )
    .await;
    transition_project_status(&app, &jwt, &project_id, "InProgress").await;
    // 项目本身停滞（3h 无活动），但刚刚跟进过 → ④ 应跳过
    backdate_project_activity(ctx.db_pool(), &project_id).await;
    set_last_followup_at(ctx.db_pool(), &project_id, now_ms()).await;

    AgentRuntimeStateManager::global().set_idle(&agent_id);

    let consumer = CronTriggerConsumer::new();
    consumer
        .on_event(
            RequestContext::new_system(),
            build_trigger_event("test-trigger-throttle", "test throttle trigger"),
        )
        .await
        .expect("on_event project_followup should succeed");

    let messages = message::domain()
        .management()
        .list_by_project_id(ctx, &project_id)
        .await
        .expect("list_by_project_id should succeed");

    let followups: Vec<_> = messages
        .iter()
        .filter(|m| m.po.message_type == MessageType::ProjectFollowupNotification)
        .collect();
    assert!(
        followups.is_empty(),
        "距上次跟进不足最小间隔时不应投递跟进通知（实际 {} 条）",
        followups.len()
    );
}

/// ⑤ 去重：项目已有未处理（`Pending`）的跟进通知时，再次触发**不**重复投递。
///
/// 回归「Agent 长时间占线（一次巡检跑 20 分钟）时，每个 tick 都往队列里再叠一条
/// 同项目的跟进通知」的堆积问题 —— 运行态预检查看不到「已投递但还没轮到」的消息。
#[sqlx::test]
async fn test_cron_project_followup_dedups_pending_notification(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FollowupDedup-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("FollowupDedupProject-{}", uuid::Uuid::now_v7()),
        Some(&agent_id),
    )
    .await;
    transition_project_status(&app, &jwt, &project_id, "InProgress").await;
    backdate_project_activity(ctx.db_pool(), &project_id).await;

    AgentRuntimeStateManager::global().set_idle(&agent_id);

    // 预先插入一条 Pending 的跟进通知（模拟「上一条还堵在队列里」）
    message::domain()
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: &bs.user_id,
                from_role: MessageRole::User,
                to_agent_id: &agent_id,
                content: "pre-existing pending followup",
                project_id: Some(&project_id),
                task_id: None,
                reply_to_id: None,
                external_key: None,
                thread_id: None,
                attachment_ids: None,
                message_type: MessageType::ProjectFollowupNotification,
            },
        )
        .await
        .expect("pre-insert pending followup should succeed");

    // 触发一次 → 应因已有 Pending 跟进通知而跳过（不新增）
    let consumer = CronTriggerConsumer::new();
    consumer
        .on_event(
            RequestContext::new_system(),
            build_trigger_event("test-trigger-project-followup-dedup", "test dedup trigger"),
        )
        .await
        .expect("on_event project_followup should succeed");

    let messages = message::domain()
        .management()
        .list_by_project_id(ctx, &project_id)
        .await
        .expect("list_by_project_id should succeed");

    let followups: Vec<_> = messages
        .iter()
        .filter(|m| {
            m.po.to_id == agent_id && m.po.message_type == MessageType::ProjectFollowupNotification
        })
        .collect();
    assert_eq!(
        followups.len(),
        1,
        "已有 Pending 的跟进通知时，本次触发不应再投递（实际 {} 条）",
        followups.len()
    );
}

/// 在项目内创建一个任务并流转到 Completed（Pending → InProgress → Completed）。
async fn create_task_and_complete(
    app: &TestApp,
    jwt: &str,
    project_id: &str,
    assignee_agent_id: &str,
    title: &str,
) {
    let task_req = json!({
        "title": title,
        "description": "task for finalize followup test",
        "project_id": project_id,
        "assignee_id": assignee_agent_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/tasks", &task_req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    let task_id = data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing task id")
        .to_string();

    for next in ["InProgress", "Completed"] {
        let req = json!({ "id": task_id, "status": next });
        let (code, body) = app
            .put_with_jwt(&format!("/api/v1/tasks/{}/status", task_id), &req, jwt)
            .await;
        assert_eq!(
            code,
            axum::http::StatusCode::OK,
            "task transition to {} should succeed, body: {}",
            next,
            body
        );
    }
}

/// ⑥ 变体（收尾分支）：项目 InProgress 但**全部任务已 Completed**
/// → 巡检仍投递跟进通知，但内容切换为「收尾」语义（二选一：关项目汇报 / 建新任务），
/// 而非「催办进度」。（含 Cancelled 的终态混合组合由 `all_tasks_terminal` 单测覆盖；
/// Task 取消不走状态接口，需专门 action，集成用例不引入该依赖。）
#[sqlx::test]
async fn test_cron_project_followup_sends_finalize_content_when_all_tasks_terminal(
    pool: SqlitePool,
) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FollowupFinalize-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("FollowupFinalizeProject-{}", uuid::Uuid::now_v7()),
        Some(&agent_id),
    )
    .await;
    transition_project_status(&app, &jwt, &project_id, "InProgress").await;

    // 2 个任务全部 Completed → all_tasks_terminal 命中
    create_task_and_complete(&app, &jwt, &project_id, &agent_id, "任务一").await;
    create_task_and_complete(&app, &jwt, &project_id, &agent_id, "任务二").await;

    // 项目行拨到 3h 前（任务写路径不更新父项目 updated_at，停滞判据看的是项目行）
    backdate_project_activity(ctx.db_pool(), &project_id).await;
    AgentRuntimeStateManager::global().set_idle(&agent_id);

    let consumer = CronTriggerConsumer::new();
    consumer
        .on_event(
            RequestContext::new_system(),
            build_trigger_event("test-trigger-finalize", "test finalize trigger"),
        )
        .await
        .expect("on_event project_followup should succeed");

    let messages = message::domain()
        .management()
        .list_by_project_id(ctx, &project_id)
        .await
        .expect("list_by_project_id should succeed");

    let followup = messages
        .iter()
        .find(|m| {
            m.po.to_id == agent_id && m.po.message_type == MessageType::ProjectFollowupNotification
        })
        .expect("all-terminal project should still receive a followup notification");

    // 内容切换为「收尾」语义，而非「催办进度」
    assert!(
        followup.po.content.contains("项目收尾检查"),
        "全任务终态应投递收尾文案，实际：{}",
        followup.po.content
    );
    assert!(!followup.po.content.contains("项目进度定期检查"));
    assert!(
        followup
            .po
            .content
            .contains("update_project_status(Completed)")
    );
    assert!(followup.po.content.contains("create_task"));
}
