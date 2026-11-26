//! Integration test for `TaskEventConsumer` (AOP async `task.status.changed`).
//!
//! Owner 通知的语义：**机械性兜底，只做确定性去重，不做「谁在忙」的猜测**。
//! - **baseline**: owner idle、项目无人忙、无 Pending 跟进 → 恰好一条 `TaskCompleted`（完成知会）。
//! - **别人忙**: 项目内有 Busy 的第三方 Agent → **仍然通知**（旧门禁 ② 已删除）。
//! - **assignee 自抑制回归**: 唯一 Busy 者就是事件 assignee（`ConsumeMode::Async`
//!   的常态窗口）→ 仍然通知。
//! - **owner 不可用**: Owner 自身 Busy → **仍然落库**（旧门禁 ③ 已删除，
//!   改由滞留重投等它空闲）。
//! - **⑤**: 该项目已有 Pending 的 `ProjectFollowupNotification` → 不再发
//!   `TaskCompleted` 知会（同项目同收件人在途即合并，唯一保留的门禁）。
//! - **滞留重投**: 早该处理却仍 `Pending` 的跟进通知能被扫出并重投；
//!   已被处理的（`Processed`）不会再投；队列里还有它的事件时也不重复投递。
//! - **Processing 双口径**: 已派发的消息 —— 去重**算它在途**，重投**捞不到它**。
//!   （去重要宽、重投要窄，见 `MessageStatus::is_unfinished` 的对照表。）
//!
//! The event JSON is built by hand (fields per
//! `src/models/events/task_status.rs::TaskStatusChangedEvent`). `TaskStatus`
//! serializes as its PascalCase variant name, so `"Completed"` deserializes to
//! `TaskStatus::Completed`.

#[path = "../common/mod.rs"]
mod common;

extern crate common as common_ext;

use crate::common::TestApp;
use ai_orz::consumer::task_completion_consumer::TaskCompletionConsumer;
use ai_orz::consumer::task_event_consumer::TaskEventConsumer;
use ai_orz::pkg::RequestContext;
use ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager;
use ai_orz::pkg::aop::Consumer;
use ai_orz::service::dal::message::RepublishOutcome;
use ai_orz::service::domain::message;
use ai_orz::service::domain::message::SendToAgentCommand;
use common_ext::enums::{MessageRole, MessageStatus, MessageType};
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;

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

/// 手工构造 `task.status.changed` 事件（`new_status = Completed`）。
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

/// 统计该项目内、发往指定 Agent 的任务完成知会（`TaskCompleted`）条数。
///
/// ⚠️ 2026-11-15：完成交付/知会已迁至 `TaskCompletionConsumer`(Sync)，类型统一为
/// `TaskCompleted`；旧的 `TaskDispatchNotification` 是项目调度指令，不再是完成回执。
/// helper 按 `to_id` 过滤，所以给 `root_user_id` 的交付消息不会被算进来。
async fn count_completion_notices(
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
        .filter(|m| m.po.to_id == owner_agent_id && m.po.message_type == MessageType::TaskCompleted)
        .count()
}

/// baseline：owner idle、项目内无 Busy Agent、无 Pending 跟进通知 →
/// 项目 Owner Agent 收到一条 `TaskCompleted` 知会。
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

    TaskCompletionConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_completion_notices(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 1,
        "baseline：owner 应收到一条 TaskCompleted 知会（实际 {} 条）",
        count
    );
}

/// 回归锚点：项目内有**除事件 assignee 以外**的 Busy Agent 时，Owner 通知仍必须发出。
///
/// 曾经这里断言的是「跳过」（旧门禁 ②：别人在忙 ⇒ 项目已在推进 ⇒ 通知冗余）。
/// 该假设已废弃 —— 忙碌来源可能是与本项目无关的事，忙完也未必产生新的任务状态
/// 事件，于是 Owner 再也等不到下一轮触发，项目静默停在原地。
/// 机械性兜底通知不该被「可能有人处理过」抑制。
#[sqlx::test]
async fn test_task_completed_notifies_owner_when_other_agent_busy(pool: SqlitePool) {
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

    TaskCompletionConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_completion_notices(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 1,
        "项目内有除 assignee 外的 Busy Agent，也不应抑制 Owner 兜底通知（实际 {} 条）",
        count
    );

    // 全局单例跨用例复用：断言后必须复位，避免串味
    AgentRuntimeStateManager::global().set_idle(&third_party_agent_id);
}

/// 回归（核心）：事件的 assignee 就是项目内**唯一** Busy 的 Agent ——
/// 正是 `ConsumeMode::Async` 消费落在「刚完成者仍 Busy」窗口内的常态。
/// 通知必须照发（旧门禁 ② 曾把它当「项目正自行推进」，靠排除 assignee 打补丁，
/// 现已整道删除）。
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

    TaskCompletionConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_completion_notices(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 1,
        "唯一 Busy 者就是事件 assignee，通知仍应发出（实际 {} 条）",
        count
    );

    // 全局单例跨用例复用：断言后必须复位，避免串味
    AgentRuntimeStateManager::global().set_idle(&assignee_agent_id);
}

/// 回归锚点：Owner 自身运行态不可用（Busy/Resting）时，通知仍必须**落库**。
///
/// 曾经这里断言的是「跳过」（旧门禁 ③：发出去也只会 nack 堆积）。问题是「跳过」
/// 等于**永久丢弃** —— 通知根本不存在了，Owner 空下来也无从得知。现在的语义是
/// 「照发」：消息落库为 `Pending`，队列重投失败后仍停在 `Pending`，由
/// `scheduler::handle_pending_notification_redelivery` 等接收方空闲时重投。
#[sqlx::test]
async fn test_task_completed_notifies_owner_even_when_owner_unavailable(pool: SqlitePool) {
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

    // owner 自身 Busy，但**不在**该项目上下文（project_id = None）——
    // 这正是旧门禁 ③ 唯一能覆盖的形态（沉淀 Resting / 被别的项目占用）。
    // 现在不再跳过：通知照发，占线由滞留重投兜底。
    AgentRuntimeStateManager::global().set_busy(&owner_agent_id, "m1", None, None);

    let task_id = format!("task-{}", uuid::Uuid::now_v7());
    let event = build_completed_event(&task_id, "已完成的任务", &project_id, &owner_agent_id);

    TaskCompletionConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_completion_notices(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 1,
        "Owner 运行态不可用也必须落库通知（改由滞留重投兜底），实际 {} 条",
        count
    );

    AgentRuntimeStateManager::global().set_idle(&owner_agent_id);
}

/// ⑤ 跨类型去重：owner 已有 Pending 的 `ProjectFollowupNotification` →
/// 本次不再发第二条 `TaskCompleted` 知会（同项目同收件人在途即合并）。
#[sqlx::test]
async fn test_completion_delivery_ignores_pending_followup_gate(pool: SqlitePool) {
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
                external: Default::default(),
                attachment_ids: None,
                message_type: MessageType::ProjectFollowupNotification,
            },
        )
        .await
        .expect("pre-insert pending followup should succeed");

    let task_id = format!("task-{}", uuid::Uuid::now_v7());
    let event = build_completed_event(&task_id, "已完成的任务", &project_id, &owner_agent_id);

    TaskCompletionConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_completion_notices(ctx.clone(), &project_id, &owner_agent_id).await;
    assert_eq!(
        count, 1,
        "完成交付是必达终点消息，不能被在途的 ProjectFollowupNotification 去重门禁吃掉（实际 {} 条）",
        count
    );
}

// ==================== 回归护栏（2026-10 静默断链）====================

/// `task_event` 队列里等待处理的 `TaskStatusChangedEvent` 条数。
fn pending_task_events() -> usize {
    ai_orz::pkg::aop::registry()
        .queue_stats("task_event")
        .map(|s| s.pending_count)
        .unwrap_or(0)
}

/// 在项目内创建任务并流转到 Completed（Pending → InProgress → Completed）。
///
/// 走 HTTP 状态接口而不是直接写库：这正是生产路径（`update_task_status` →
/// `TaskManageImpl::transition_status` → `TaskDALImpl::update`），也是事件曾经丢失的地方。
async fn create_task_and_complete(
    app: &TestApp,
    jwt: &str,
    project_id: &str,
    assignee_agent_id: &str,
    title: &str,
) -> String {
    let task_req = json!({
        "title": title,
        "description": "task for status-event regression",
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
    task_id
}

/// 生产者回归：任务经状态接口走到 Completed 时，`TaskStatusChangedEvent` 必须真的入队。
///
/// 这条用例存在的理由：本文件其余用例都是**手工构造事件**直接喂 `on_event`，
/// 只验证了消费端。「生产端有没有发事件」完全没有覆盖 —— 2026-10 的实证正是生产端
/// 静默缺失：`messages` 表 `type=10` 零条、消费者零条日志，而消费端用例全绿。
///
/// 注意必须先把 `TaskEventConsumer` 注册进 registry：`publish` 对**没有订阅者**的
/// topic 直接丢弃（静默 return），不注册就永远测不出「发没发」。
#[sqlx::test]
async fn test_task_completed_publishes_status_changed_event(pool: SqlitePool) {
    let _serial = SERIAL.lock().await;
    let _ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 同进程只注册一次（registry 是全局单例）
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        let _ = ai_orz::pkg::aop::registry().register_consumer(Arc::new(TaskEventConsumer::new()));
    });

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventProducer-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventProducerProject-{}", uuid::Uuid::now_v7()),
        Some(&agent_id),
    )
    .await;

    let before = pending_task_events();
    let task_id =
        create_task_and_complete(&app, &jwt, &project_id, &agent_id, "发布探针任务").await;
    let after = pending_task_events();

    assert!(
        after > before,
        "任务 {} 完成后必须发布 TaskStatusChangedEvent（队列 {} → {}）—— \
         生产端不发事件时，Owner 通知与后继补发整条链路都是死的",
        task_id,
        before,
        after
    );
}

// ==================== 交付责任制分层（2026-11-14）====================

/// 带交付身份快照的完成事件（project_id 可为 null，表达游离任务）。
fn build_completed_event_with_delivery(
    task_id: &str,
    project_id: Option<&str>,
    root_user_id: &str,
    initiator_agent_id: &str,
    trigger_message_id: &str,
) -> serde_json::Value {
    json!({
        "event_id": uuid::Uuid::now_v7().to_string(),
        "task_id": task_id,
        "task_title": "分层交付任务",
        "project_id": project_id,
        "assignee_id": initiator_agent_id,
        "old_status": "InProgress",
        "new_status": "Completed",
        "progress": 100,
        "created_at": now_ms(),
        "root_user_id": root_user_id,
        "initiator_agent_id": initiator_agent_id,
        "trigger_message_id": trigger_message_id,
    })
}

/// 无项目任务完成：完成知会必须发给任务发起者、类型必须是 `TaskCompleted`，
/// 且挂回触发消息链（不能另起新链）。
///
/// 旧实现 `let Some(project_id) = ... else { return Ok(()) }` 在这里直接吞掉
/// 整个事件 —— 游离任务完成后没有任何 Agent 被唤醒，归属用户永远收不到结果。
#[sqlx::test]
async fn test_free_task_completion_notifies_initiator_with_task_completed(pool: SqlitePool) {
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let initiator = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FreeTaskInitiator-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let task_id = format!("free-task-{}", uuid::Uuid::now_v7());
    let trigger_id = format!("trigger-{}", uuid::Uuid::now_v7());
    let event =
        build_completed_event_with_delivery(&task_id, None, &bs.user_id, &initiator, &trigger_id);

    TaskCompletionConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let messages = ai_orz::service::dal::message::dal()
        .query(
            ctx.clone(),
            ai_orz::service::dao::message::MessageQuery {
                task_id: Some(task_id.clone()),
                ..Default::default()
            },
        )
        .await
        .expect("query should succeed");
    let completed: Vec<_> = messages
        .iter()
        .filter(|m| m.po.message_type == MessageType::TaskCompleted)
        .collect();

    // 交付责任制（2026-11-15）：无项目任务完成产出**两条**各司其职的消息 ——
    // 一条带关键字段/执行结果/产物清点的交付回执直达归属用户，一条知会给发起者。
    let delivery = completed
        .iter()
        .find(|m| m.po.to_id == bs.user_id)
        .expect("归属用户必须收到一条直达的交付回执");
    assert!(
        delivery.po.content.contains("任务完成回执")
            && delivery.po.content.contains("交付产物")
            && delivery.po.content.contains(&task_id),
        "交付回执必须带任务维度关键字段，实际正文：{}",
        delivery.po.content
    );
    assert!(
        delivery.po.content.contains("无项目任务不落产物"),
        "artifacts.project_id NOT NULL ⇒ 游离任务没有产物通道，正文必须显式降级说明"
    );
    assert!(
        delivery.po.reply_to_id.is_some(),
        "交付回执必须挂回触发消息链"
    );

    let notice = completed
        .iter()
        .find(|m| m.po.to_id == initiator)
        .expect("任务发起者必须收到一条完成知会");
    assert!(
        notice.po.content.contains("直接送达归属用户"),
        "知会必须写明交付已直达用户，避免收件方再转发一份"
    );
    assert!(notice.po.reply_to_id.is_some(), "知会同样挂回同一条链");
}

/// 项目存在但未配置 owner：完成知会必须退回任务发起者，不能静默 return。
/// 前端创建项目恒传 `owner_agent_id: None`，这是线上默认形态。
#[sqlx::test]
async fn test_project_owner_missing_falls_back_to_initiator(pool: SqlitePool) {
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let initiator = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FallbackInitiator-{}", uuid::Uuid::now_v7()),
    )
    .await;
    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("NoOwnerProject-{}", uuid::Uuid::now_v7()),
        None,
    )
    .await;

    let task_id = format!("no-owner-task-{}", uuid::Uuid::now_v7());
    let event = build_completed_event_with_delivery(
        &task_id,
        Some(&project_id),
        "test-root-user",
        &initiator,
        "",
    );

    TaskCompletionConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("on_event should succeed");

    let count = count_completion_notices(ctx.clone(), &project_id, &initiator).await;
    assert_eq!(
        count, 1,
        "owner 未配置时完成知会应退回任务发起者（实际 {} 条）",
        count
    );
}

/// `create_task` 走「动作 → AOP → TaskAssignment」后必须真的落库一条派单消息。
///
/// ⚠️ 这条用例同时守住 **Sync 消费者已注册** 这个前提：`aop::publish` 对没有
/// 订阅者的 topic 是静默 return（连日志都不打），漏注册 = 任务建好了但永远
/// 没有 Agent 被唤醒，而且不会有任何报错。
#[sqlx::test]
async fn test_create_task_dispatches_assignment_via_aop(pool: SqlitePool) {
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("AopDispatchTarget-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let task_req = json!({
        "title": "AOP 派发探针",
        "description": "验证派单消息经 Sync 消费者内联落库",
        "assignee_id": agent_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/tasks", &task_req, &jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    let task_id = data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing task id")
        .to_string();

    let messages = ai_orz::service::dal::message::dal()
        .query(
            ctx.clone(),
            ai_orz::service::dao::message::MessageQuery {
                task_id: Some(task_id.clone()),
                ..Default::default()
            },
        )
        .await
        .expect("query should succeed");
    let assignments: Vec<_> = messages
        .iter()
        .filter(|m| m.po.message_type == MessageType::TaskAssignment)
        .collect();

    assert_eq!(
        assignments.len(),
        1,
        "任务 {} 创建后必须恰好有一条 TaskAssignment（实际 {} 条）—— \
         没有就说明 task.dispatched 的 Sync 消费者没注册或派发失败",
        task_id,
        assignments.len()
    );
    assert_eq!(
        assignments[0].po.to_id, agent_id,
        "派单必须直达 task.assignee_id，owner 只做知会不做转发"
    );
}

/// 去重作用域回归：Pending 跟进通知的压制**只在同一项目内生效**。
///
/// Owner 同时带两个项目时，按 Agent 去重会让先挂上 Pending 的项目永远占着门，
/// 另一个项目的任务完成通知被饿死。这里用「A 有 Pending、B 没有」直接钉住作用域：
/// A 被去重（0 条），B 必须照常通知（1 条）。
#[sqlx::test]
async fn test_completion_notice_is_scoped_to_project(pool: SqlitePool) {
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let owner_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventScopeOwner-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // 同一个 Owner 带两个项目 —— 正是「按 Agent 去重」会出事的形态
    let project_a = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventScopeA-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;
    let project_b = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventScopeB-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;

    AgentRuntimeStateManager::global().set_idle(&owner_agent_id);

    // 只有项目 A 已有 Pending 跟进通知
    message::domain()
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: &bs.user_id,
                from_role: MessageRole::User,
                to_agent_id: &owner_agent_id,
                content: "pending followup on project A",
                project_id: Some(&project_a),
                task_id: None,
                reply_to_id: None,
                external: Default::default(),
                attachment_ids: None,
                message_type: MessageType::ProjectFollowupNotification,
            },
        )
        .await
        .expect("pre-insert pending followup should succeed");

    // A：被同项目去重拦下
    TaskCompletionConsumer::new()
        .on_event(
            RequestContext::new_system(),
            build_completed_event(
                &format!("task-a-{}", uuid::Uuid::now_v7()),
                "A 项目任务",
                &project_a,
                &owner_agent_id,
            ),
        )
        .await
        .expect("on_event should succeed");
    let count_a = count_completion_notices(ctx.clone(), &project_a, &owner_agent_id).await;
    assert_eq!(
        count_a, 1,
        "项目 A 即使已有 Pending 跟进通知，完成知会也必须送达（实际 {} 条）",
        count_a
    );

    // B：不受项目 A 的在途通知影响
    TaskCompletionConsumer::new()
        .on_event(
            RequestContext::new_system(),
            build_completed_event(
                &format!("task-b-{}", uuid::Uuid::now_v7()),
                "B 项目任务",
                &project_b,
                &owner_agent_id,
            ),
        )
        .await
        .expect("on_event should succeed");
    let count_b = count_completion_notices(ctx.clone(), &project_b, &owner_agent_id).await;
    assert_eq!(
        count_b, 1,
        "项目 B 没有在途跟进通知，任务完成必须通知 Owner（实际 {} 条）—— \
         按 Agent 去重会让 A 的 Pending 把 B 饿死",
        count_b
    );
}

/// 完成知会按**项目**合并：同一项目连续完成多个任务，Owner 只保留一条在途知会；
/// 那条被消费掉（`Processed`）之后，下一次完成才再发一条。
///
/// 反例（2026-11-15 回归）：职责从 `TaskEventConsumer` 迁到
/// `TaskCompletionConsumer` 时项目级去重丢了 ⇒ N 次完成 = N 轮完整思维链；
/// `ReplyObligation` 的 NotifyOnly 只让 Final 被 Discard，**拦不住唤醒**。
#[sqlx::test]
async fn test_completion_notice_coalesces_within_project(pool: SqlitePool) {
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let owner_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventCoalesce-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventCoalesceProject-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;

    AgentRuntimeStateManager::global().set_idle(&owner_agent_id);

    let consumer = TaskCompletionConsumer::new();

    // ① 第一个任务完成 → 一条知会
    consumer
        .on_event(
            RequestContext::new_system(),
            build_completed_event(
                &format!("task-1-{}", uuid::Uuid::now_v7()),
                "第一个任务",
                &project_id,
                &owner_agent_id,
            ),
        )
        .await
        .expect("on_event should succeed");
    assert_eq!(
        count_completion_notices(ctx.clone(), &project_id, &owner_agent_id).await,
        1,
        "第一个任务的完成知会必须送达"
    );

    // ② 同项目第二个任务完成 → 第一条仍在途，应合并（不再唤醒一轮）
    consumer
        .on_event(
            RequestContext::new_system(),
            build_completed_event(
                &format!("task-2-{}", uuid::Uuid::now_v7()),
                "第二个任务",
                &project_id,
                &owner_agent_id,
            ),
        )
        .await
        .expect("on_event should succeed");
    assert_eq!(
        count_completion_notices(ctx.clone(), &project_id, &owner_agent_id).await,
        1,
        "同一项目已有在途完成知会，第二条必须合并（实际多发了就是 N 次完成 N 轮唤醒）"
    );

    // ③ 消费侧把那条知会处理完 → 第三条才能再发（合并，不是永久静音）
    let notice = message::domain()
        .management()
        .list_by_project_id(ctx.clone(), &project_id)
        .await
        .expect("list_by_project_id should succeed")
        .into_iter()
        .find(|m| m.po.to_id == owner_agent_id && m.po.message_type == MessageType::TaskCompleted)
        .expect("在途完成知会必须存在");
    ai_orz::service::dal::message::dal()
        .update_status(ctx.clone(), &notice.po.id, MessageStatus::Processed)
        .await
        .expect("update_status should succeed");

    consumer
        .on_event(
            RequestContext::new_system(),
            build_completed_event(
                &format!("task-3-{}", uuid::Uuid::now_v7()),
                "第三个任务",
                &project_id,
                &owner_agent_id,
            ),
        )
        .await
        .expect("on_event should succeed");
    assert_eq!(
        count_completion_notices(ctx.clone(), &project_id, &owner_agent_id).await,
        2,
        "上一条知会处理完后，新的完成必须再发一条（否则 Owner 会漏掉后续交付）"
    );
}

/// 滞留重投：早该处理却仍停在 `Pending` 的跟进通知能被扫出并重投；
/// 已处理（`Processed`）的不会再被投一次。
///
/// 这是「删掉『谁在忙』门禁」之后必须存在的另一半：通知不再被丢弃，但投递时
/// 若接收方持续占线，AOP 退避重投会以 `Discard` 收场，消息就永久停在 `Pending`。
/// 消息表是权威待办 —— 这道扫描是它唯一的消费者。
#[sqlx::test]
async fn test_stale_pending_notification_redelivery(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let owner_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventStale-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventStaleProject-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;

    let send = |content: &'static str| {
        let ctx = ctx.clone();
        let owner = owner_agent_id.clone();
        let project = project_id.clone();
        async move {
            message::domain()
                .delivery()
                .send_to_agent(
                    ctx,
                    SendToAgentCommand {
                        from_id: "system",
                        from_role: MessageRole::System,
                        to_agent_id: &owner,
                        content,
                        project_id: Some(&project),
                        task_id: None,
                        reply_to_id: None,
                        external: Default::default(),
                        attachment_ids: None,
                        message_type: MessageType::TaskDispatchNotification,
                    },
                )
                .await
                .expect("send_to_agent should succeed")
        }
    };

    let fresh = send("刚发出的通知（仍在正常消费窗口内）").await;
    let stale = send("十六分钟前就该处理的通知").await;

    // 把 stale 的创建时间推到 16 分钟前：模拟「投递后重投全部失败、永久停在 Pending」
    //
    // ⚠️ 必须打 `ctx.db_pool()` 而不是用例参数 `pool`：全局 DAL 单例在本文件首个
    // 用例初始化时就固定在那一刻的库上（`init_full_test_env` 的 OnceCell 语义），
    // 后续用例传入的是另一个临时库 —— 用 `pool` 会更新到一个没人读的副本上。
    sqlx::query("UPDATE messages SET created_at = ? WHERE id = ?")
        .bind(now_ms() - 16 * 60 * 1000)
        .bind(&stale.po.id)
        .execute(ctx.db_pool())
        .await
        .expect("shift created_at should succeed");

    let found = message::domain()
        .list_stale_pending_notifications(ctx.clone(), 15 * 60 * 1000, 200)
        .await
        .expect("list_stale_pending_notifications should succeed");
    let ids: Vec<&str> = found.iter().map(|m| m.po.id.as_str()).collect();
    assert!(
        ids.contains(&stale.po.id.as_str()),
        "16 分钟前仍 Pending 的跟进通知必须被判为滞留（实际扫出 {} 条）",
        ids.len()
    );
    assert!(
        !ids.contains(&fresh.po.id.as_str()),
        "刚创建的通知可能正在被消费，不能判为滞留（否则会重复唤醒）"
    );

    let dal = ai_orz::service::dal::message::dal();

    // ① 「事件还在队列里」⇒ AlreadyQueued，**不得**重复投递。
    //    这正是集成环境的常态：`send_to_agent` 会 publish 事件，而测试里没人出队，
    //    所以它确实还在队列里 —— 它不是丢了的那个样子。
    assert_eq!(
        dal.republish_created(ctx.clone(), &stale.po.id)
            .await
            .expect("republish_created should succeed"),
        RepublishOutcome::AlreadyQueued,
        "事件仍在队列中 ⇒ 必须返回 AlreadyQueued，不得再发布一份（队列防重）"
    );

    // ② 「事件真的丢了」才是要救的场景：消息在库里 Pending，队列里却没有它
    //    （进程重启导致内存队列清空 / publish 时无订阅者 / on_consumed 写库失败）。
    //    直接插行绕过 `save_message` ⇒ 不经过 publish ⇒ 队列里不会有它的事件。
    //    注：用 `SELECT` 复制既有行的其余字段（`file_meta` 是结构体，手写 JSON 容易
    //    漏字段导致 decode 失败），只覆盖 id / content / created_at / updated_at。
    let lost_id = format!("lost-event-{}", uuid::Uuid::now_v7());
    let old_created_at = now_ms() - 16 * 60 * 1000;
    sqlx::query(
        "INSERT INTO messages \
         (id, project_id, task_id, from_id, to_id, from_role, to_role, message_type, file_type, \
          status, content, file_meta, reply_to_id, organization_id, root_id, thread_id, \
          external_key, created_by, modified_by, created_at, updated_at) \
         SELECT ?, project_id, task_id, from_id, to_id, from_role, to_role, message_type, \
                file_type, ?, ?, file_meta, reply_to_id, organization_id, root_id, thread_id, \
                external_key, created_by, modified_by, ?, ? \
         FROM messages WHERE id = ?",
    )
    .bind(&lost_id)
    .bind(MessageStatus::Pending.to_i32())
    .bind("事件丢失的通知（模拟进程重启后内存队列清空）")
    .bind(old_created_at)
    .bind(old_created_at)
    .bind(&stale.po.id)
    .execute(ctx.db_pool())
    .await
    .expect("insert lost message should succeed");

    assert_eq!(
        dal.republish_created(ctx.clone(), &lost_id)
            .await
            .expect("republish_created should succeed"),
        RepublishOutcome::Redelivered,
        "Pending 但队列里没有它的事件 ⇒ 这才是真丢了的场景，必须重投"
    );
    assert_eq!(
        dal.republish_created(ctx.clone(), &lost_id)
            .await
            .expect("republish_created should succeed"),
        RepublishOutcome::AlreadyQueued,
        "刚重投过 ⇒ 事件已在队列中，重复调用不得产生第二份投递"
    );

    dal.update_status(ctx.clone(), &lost_id, MessageStatus::Processed)
        .await
        .expect("update_status should succeed");
    assert_eq!(
        dal.republish_created(ctx.clone(), &lost_id)
            .await
            .expect("republish_created should succeed"),
        RepublishOutcome::NotPending,
        "已处理（Processed）的消息不得再重投，否则同一通知会唤醒两次"
    );
}

/// `Processing` 属于「已派发」：**既不参与去重放行、也不参与重投**
///
/// 这是最容易写反的一条：`Processing` 表示消费侧已把消息领走（链路还在跑）。
/// - 去重必须**算它在途**（否则重复派发同一任务）；
/// - 重投必须**捞不到它**（否则同一条消息被并发消费两次）。
///
/// 本用例把这两半同时锁住，任何一半写错都会在这里红。
#[sqlx::test]
async fn test_processing_is_inflight_for_dedup_but_not_for_redelivery(pool: SqlitePool) {
    // 串行化临界区：见文件顶部 SERIAL 说明
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool.clone()).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let owner_agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("TaskEventProcessing-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("TaskEventProcessingProject-{}", uuid::Uuid::now_v7()),
        Some(&owner_agent_id),
    )
    .await;

    let notification = message::domain()
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: "system",
                from_role: MessageRole::System,
                to_agent_id: &owner_agent_id,
                content: "跟进通知",
                project_id: Some(&project_id),
                task_id: None,
                reply_to_id: None,
                external: Default::default(),
                attachment_ids: None,
                message_type: MessageType::TaskDispatchNotification,
            },
        )
        .await
        .expect("send_to_agent should succeed");

    let dal = ai_orz::service::dal::message::dal();

    // 把创建时间推到 16 分钟前，让它同时满足「滞留时间窗」
    sqlx::query("UPDATE messages SET created_at = ? WHERE id = ?")
        .bind(now_ms() - 16 * 60 * 1000)
        .bind(&notification.po.id)
        .execute(ctx.db_pool())
        .await
        .expect("shift created_at should succeed");

    // 模拟消费侧刚把它领走（`Producer::on_started` 的写入）
    dal.update_status(ctx.clone(), &notification.po.id, MessageStatus::Processing)
        .await
        .expect("update_status should succeed");

    // ① 重投不得捞 Processing —— 它已经在飞，捞回来会造成并发消费
    let found = message::domain()
        .list_stale_pending_notifications(ctx.clone(), 15 * 60 * 1000, 200)
        .await
        .expect("list_stale_pending_notifications should succeed");
    assert!(
        !found.iter().any(|m| m.po.id == notification.po.id),
        "Processing 表示已被领走，滞留扫描必须跳过它（否则同一消息被消费两次）"
    );
    assert_eq!(
        dal.republish_created(ctx.clone(), &notification.po.id)
            .await
            .expect("republish_created should succeed"),
        RepublishOutcome::NotPending,
        "Processing 不得被重投"
    );

    // ② 但去重必须算它在途 —— 否则同一项目的跟进通知会被重复投
    assert!(
        message::domain()
            .has_pending_followup_for_project(ctx.clone(), &project_id)
            .await
            .expect("has_pending_followup_for_project should succeed"),
        "Processing 的跟进通知仍在途，去重必须命中（否则重复投同一项目的跟进）"
    );
}

/// 验收层红线（2026-11-15）：`PendingReview` **不是** `Completed`。两个以
/// `new_status == Completed` 为门禁的消费者必须整条短路 —— 不发交付、不解锁
/// DAG 后继。否则执行人一提交，用户就收到「已完成」回执、下游也提前开工，
/// Owner 的验收判定形同虚设。
#[sqlx::test]
async fn pending_review_event_does_not_trigger_delivery_or_dag(pool: SqlitePool) {
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let project_id = format!("proj-{}", uuid::Uuid::now_v7());
    let task_id = format!("task-{}", uuid::Uuid::now_v7());

    let event = json!({
        "event_id": uuid::Uuid::now_v7().to_string(),
        "task_id": task_id,
        "task_title": "待验收的任务",
        "project_id": project_id,
        "assignee_id": "agent-worker",
        "old_status": "InProgress",
        "new_status": "PendingReview",
        "progress": 90,
        "created_at": now_ms(),
        "root_user_id": "user-1",
        "initiator_agent_id": "agent-reception",
        "trigger_message_id": "",
    });

    TaskCompletionConsumer::new()
        .on_event(ctx.clone(), event.clone())
        .await
        .expect("completion consumer should short-circuit PendingReview");
    TaskEventConsumer::new()
        .on_event(ctx.clone(), event)
        .await
        .expect("dag consumer should short-circuit PendingReview");

    let messages = message::domain()
        .management()
        .list_by_project_id(ctx.clone(), &project_id)
        .await
        .expect("list_by_project_id should succeed");
    assert!(
        messages.is_empty(),
        "提交验收不得产出任何交付回执 / 后继派单（实际 {} 条）",
        messages.len()
    );
}

/// 待验收知会必须进「滞留通知重投」白名单：它一旦投递丢失，任务会永久停在
/// 待验收且没有任何报错（交付与 DAG 都只认 Completed）。定时重发知会是这条
/// 链路最后的确定性兜底 —— 注意只重发，不自动确认。
#[sqlx::test]
async fn pending_review_notice_is_eligible_for_stale_redelivery(pool: SqlitePool) {
    let _serial = SERIAL.lock().await;
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let project_id = format!("proj-{}", uuid::Uuid::now_v7());
    let agent_id = format!("agent-{}", uuid::Uuid::now_v7());

    let notice = message::domain()
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: "system",
                from_role: MessageRole::System,
                to_agent_id: &agent_id,
                content: "请验收该任务",
                project_id: Some(&project_id),
                task_id: None,
                reply_to_id: None,
                external: Default::default(),
                attachment_ids: None,
                message_type: MessageType::TaskPendingReview,
            },
        )
        .await
        .expect("send_to_agent should succeed");

    sqlx::query("UPDATE messages SET created_at = ? WHERE id = ?")
        .bind(now_ms() - 16 * 60 * 1000)
        .bind(&notice.po.id)
        .execute(ctx.db_pool())
        .await
        .expect("shift created_at should succeed");

    let found = message::domain()
        .list_stale_pending_notifications(ctx.clone(), 15 * 60 * 1000, 200)
        .await
        .expect("list_stale_pending_notifications should succeed");
    assert!(
        found.iter().any(|m| m.po.id == notice.po.id),
        "滞留的 TaskPendingReview 知会必须能被扫出重投（实际扫出 {} 条）",
        found.len()
    );
}
