//! Integration test for `TaskEventConsumer` (AOP async `task.status_changed`).
//!
//! Covers the three Owner-notification gates added on top of the unconditional
//! successor re-dispatch:
//! - **baseline**: owner idle + no busy agent on the project + no pending followup
//!   → the Owner Agent receives exactly one `TaskDispatchNotification`.
//! - **②**: a Busy runtime Agent, **other than the event's assignee**, on the same
//!   project → no notification (project is self-advancing).
//! - **② regression (self-suppression race)**: the event's assignee is the *only*
//!   Busy agent on the project — the exact `ConsumeMode::Async` window — → the
//!   owner is still notified: gate ② must exclude the event assignee.
//! - **③**: the Owner itself is Busy (on another context) → no notification.
//! - **⑤**: a Pending `ProjectFollowupNotification` already exists for the owner
//!   → no `TaskDispatchNotification` (cross-type followup dedup).
//!
//! The event JSON is built by hand (fields per
//! `src/models/events/task_status.rs::TaskStatusChangedEvent`). `TaskStatus`
//! serializes as its PascalCase variant name, so `"Completed"` deserializes to
//! `TaskStatus::Completed`.

#[path = "../common/mod.rs"]
mod common;

extern crate common as common_ext;

use crate::common::TestApp;
use ai_orz::consumer::task_event_consumer::TaskEventConsumer;
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
/// 单例），而 `TaskEventConsumer` 读取全局运行态单例且对全局有副作用 ——
/// 若不串行，A 用例设置的 Busy 状态会串进 B 用例的判定。用一把锁把每个用例的
/// 「建项目 → 置运行态 → 触发 → 断言」整体串起来，消除并发交叉。
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 当前毫秒时间戳
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 手工构造 `task.status_changed` 事件（`new_status = Completed`）。
fn build_completed_event(
    task_id: &str,
    task_title: &str,
    project_id: &str,
    assignee_id: &str,
) -> serde_json::Value {
    json!({
        "event_id": uuid::Uuid::now_v7().to_string(),
        "task_id": task_id,
        "task_title": task_title,
        "project_id": project_id,
        "assignee_id": assignee_id,
        "old_status": "InProgress",
        "new_status": "Completed",
        "progress": 100,
        "created_at": now_ms(),
    })
}

/// Create a project via HTTP API with the given `owner_agent_id` (optional).
async fn create_project_with_owner(
    app: &TestApp,
    jwt: &str,
    name: &str,
    owner_agent_id: Option<&str>,
) -> String {
    let mut req = json!({
        "name": name,
        "description": "Test project for TaskEventConsumer",
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

/// 统计该项目内、发往指定 owner 的 `TaskDispatchNotification` 条数。
async fn count_dispatch_notifications(
    ctx: RequestContext,
    project_id: &str,
    owner_agent_id: &str,
) -> usize {
    let messages = message::domain()
        .management()
        .list_by_project_id(ctx, project_id)
        .await
        .expect("list_by_project_id should succeed");
    messages
        .iter()
        .filter(|m| {
            m.po.to_id == owner_agent_id
                && m.po.message_type == MessageType::TaskDispatchNotification
        })
        .count()
}

/// baseline：owner idle、项目内无 Busy Agent、无 Pending 跟进通知 →
/// 项目 Owner Agent 收到一条 `TaskDispatchNotification`。
#[sqlx::test]
async fn test_task_completed_notifies_owner_baseline(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let owner_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventOwner-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventProject-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;

    // owner idle；项目内无 Busy Agent；无 Pending 跟进通知
    AgentRuntimeStateManager::global().set_idle(&owner_agent_id);

    let task_id = format!("task-{}", uuid::Uuid::now_v7());
    let event = build_completed_event(&task_id, "已完成的任务", &project_id, &owner_agent_id);

    TaskEventConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_dispatch_notifications(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 1,
        "baseline：owner 应收到一条 TaskDispatchNotification（实际 {} 条）",
        count
    );
}

/// ② 项目内有**除事件 assignee 以外**的 Busy 运行态 Agent → 项目正自行推进，跳过 Owner 通知。
#[sqlx::test]
async fn test_task_completed_skips_when_agent_busy_on_project(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let owner_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventBusy-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // 第三方 Agent：既非 owner、也非本次事件的 assignee
    let third_party_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventBusyThirdParty-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventBusyProject-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;

    // owner idle；第三方 Agent 在本项目上下文 Busy → 属于「除 assignee 外」的 Busy Agent
    AgentRuntimeStateManager::global().set_idle(&owner_agent_id);
    AgentRuntimeStateManager::global().set_busy(
        &third_party_agent_id,
        "m1",
        None,
        Some(&project_id),
    );

    // 事件 assignee 用 owner（非 Busy）：证明跳过是第三方 Busy 触发，而非 assignee 自抑制
    let task_id = format!("task-{}", uuid::Uuid::now_v7());
    let event = build_completed_event(&task_id, "已完成的任务", &project_id, &owner_agent_id);

    TaskEventConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_dispatch_notifications(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 0,
        "项目内有除 assignee 外的 Busy Agent 时不应通知 owner（实际 {} 条）",
        count
    );

    // 全局单例跨用例复用：断言后必须复位，避免串味
    AgentRuntimeStateManager::global().set_idle(&third_party_agent_id);
}

/// ② 回归（核心）：事件的 assignee 就是项目内**唯一** Busy 的 Agent ——
/// 正是 `ConsumeMode::Async` 消费落在「刚完成者仍 Busy」窗口内的常态。
/// 门禁 ② 必须排除该 assignee，通知 Owner 仍应发出（自抑制竞态修复的回归锚点）。
#[sqlx::test]
async fn test_task_completed_notifies_owner_when_only_assignee_busy(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let owner_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventSelfSuppressOwner-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let assignee_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventSelfSuppressAssignee-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventSelfSuppressProject-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;

    // owner idle；唯一 Busy 的 Agent 恰是本次事件的 assignee（自抑制窗口）
    AgentRuntimeStateManager::global().set_idle(&owner_agent_id);
    AgentRuntimeStateManager::global().set_busy(&assignee_agent_id, "m1", None, Some(&project_id));

    let task_id = format!("task-{}", uuid::Uuid::now_v7());
    let event = build_completed_event(&task_id, "已完成的任务", &project_id, &assignee_agent_id);

    TaskEventConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_dispatch_notifications(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 1,
        "唯一 Busy 者就是事件 assignee，门禁 ② 应排除它、通知仍应发出（实际 {} 条）",
        count
    );

    // 全局单例跨用例复用：断言后必须复位，避免串味
    AgentRuntimeStateManager::global().set_idle(&assignee_agent_id);
}

/// ③ owner 自身运行态不可用（Busy，且不在本项目上下文）→ 跳过 Owner 通知。
#[sqlx::test]
async fn test_task_completed_skips_when_owner_unavailable(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let owner_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventUnavailable-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventUnavailableProject-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;

    // owner 自身 Busy，但**不在**该项目上下文（project_id = None）→
    // 门禁 ②（项目内有 Busy Agent）不命中，门禁 ③（owner 不可用）命中
    AgentRuntimeStateManager::global().set_busy(&owner_agent_id, "m1", None, None);

    let task_id = format!("task-{}", uuid::Uuid::now_v7());
    let event = build_completed_event(&task_id, "已完成的任务", &project_id, &owner_agent_id);

    TaskEventConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_dispatch_notifications(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 0,
        "owner 运行态不可用不应通知 owner（实际 {} 条）",
        count
    );

    AgentRuntimeStateManager::global().set_idle(&owner_agent_id);
}

/// ⑤ 跨类型去重：owner 已有 Pending 的 `ProjectFollowupNotification` →
/// 本次不再发 `TaskDispatchNotification`（跟进类统一去重）。
#[sqlx::test]
async fn test_task_completed_dedups_pending_followup_of_other_type(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let owner_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventDedup-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventDedupProject-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;

    AgentRuntimeStateManager::global().set_idle(&owner_agent_id);

    // 预先插入一条 Pending 的**异类型**跟进通知（定时巡检链路），模拟「已有在途跟进」
    message::domain()
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: &bs.user_id,
                from_role: MessageRole::User,
                to_agent_id: &owner_agent_id,
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

    let task_id = format!("task-{}", uuid::Uuid::now_v7());
    let event = build_completed_event(&task_id, "已完成的任务", &project_id, &owner_agent_id);

    TaskEventConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_dispatch_notifications(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 0,
        "已有 Pending 的 ProjectFollowupNotification 时不应再发 TaskDispatchNotification（实际 {} 条）",
        count
    );
}
