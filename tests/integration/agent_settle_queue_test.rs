//! Integration test for the queued sleep-settle path.
//!
//! Regression guard for the bug where a daily `agent_rest` trigger fired, found the
//! target Agent `Busy` (typically because the hourly project follow-up had just woken
//! the *same* Agent out of the same cron poll), silently returned `Ok(0)` — and the
//! trigger still advanced `next_run_at` to the next 04:00, so nothing was settled for
//! a whole day.
//!
//! Locked behaviour:
//! 1. `CronTriggerConsumer` (`agent_rest`) only **dispatches** `agent.settle.requested`;
//!    it must enqueue a request even when the Agent is Busy, and must not touch the
//!    Agent runtime state itself.
//! 2. `AgentSettleConsumer` reports a conflict (→ framework nack/requeue) instead of
//!    dropping the request when the Agent is Busy.
//! 3. `AgentSettleConsumer` releases the Agent afterwards (no永久卡在 Resting).

#[path = "../common/mod.rs"]
mod common;

extern crate common as common_ext;

use crate::common::TestApp;
use ai_orz::consumer::agent_settle::AgentSettleConsumer;
use ai_orz::consumer::scheduler::CronTriggerConsumer;
use ai_orz::models::events::AgentSettleEvent;
use ai_orz::pkg::RequestContext;
use ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager;
use ai_orz::pkg::aop::Consumer;
use ai_orz::pkg::aop::queue::EventQueryFilter;
use serde_json::json;
use sqlx::SqlitePool;

/// Consumer name registered in `consumer::init()` (queue routing key)
const SETTLE_CONSUMER: &str = "agent_settle";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Build the AOP envelope for a `cron.trigger` event with `agent_rest` payload.
fn agent_rest_event(agent_id: &str) -> serde_json::Value {
    json!({
        "event_id": uuid::Uuid::now_v7().to_string(),
        "trigger_id": "test-trigger-agent-rest",
        "trigger_name": "test agent rest trigger",
        "payload": json!({
            "action": "agent_rest",
            "extra": {"agent_id": agent_id, "settle_limit": 5},
        })
        .to_string(),
        "created_at": now_ms(),
    })
}

/// Count queued settle requests for one Agent (order_key = agent_id).
fn settle_requests_for(agent_id: &str) -> usize {
    ai_orz::pkg::aop::registry()
        .query_events(
            SETTLE_CONSUMER,
            EventQueryFilter {
                order_key: Some(agent_id.to_string()),
                ..Default::default()
            },
        )
        .expect("agent_settle 队列应随消费者注册而创建")
        .len()
}

/// 核心回归：Agent 正忙（项目巡检刚在同一轮 cron poll 里唤醒它）时，
/// `agent_rest` 仍然必须**把沉淀请求排进队列**，而不是静默跳过。
#[sqlx::test]
async fn test_agent_rest_dispatches_settle_request_when_agent_busy(pool: SqlitePool) {
    let _ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("SettleQueueBusy-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let before = settle_requests_for(&agent_id);

    // 模拟「同一时刻多个任务唤醒同一个 Agent」：巡检先把这个 Agent 占成 Busy
    let mgr = AgentRuntimeStateManager::global();
    mgr.set_busy(
        &agent_id,
        "msg-project-followup",
        Some("task-1"),
        Some("proj-1"),
    );

    let consumer = CronTriggerConsumer::new();
    consumer
        .on_event(RequestContext::new_system(), agent_rest_event(&agent_id))
        .await
        .expect("agent_rest 只派发，不应因为 Agent 忙而失败");

    // 1) 请求确实排进了队列（旧实现在这里什么都没有 → 丢一整天）
    assert_eq!(
        settle_requests_for(&agent_id),
        before + 1,
        "Agent {} 忙也必须把沉淀请求排进 agent_settle 队列",
        agent_id
    );

    // 2) 派发方不碰状态：Agent 仍归巡检占用，上下文原样保留
    let info = mgr.get(&agent_id).expect("runtime state should exist");
    assert_eq!(
        info.current_message_id,
        Some("msg-project-followup".to_string())
    );
    assert_eq!(info.task_id, Some("task-1".to_string()));
    assert_eq!(info.project_id, Some("proj-1".to_string()));
    assert!(
        mgr.is_unavailable(&agent_id),
        "派发不应释放巡检持有的 Busy 状态"
    );

    mgr.set_idle(&agent_id);
}

/// 消费者侧契约：抢不到 Agent 时必须上报冲突，让框架 nack 重投（= 排队），
/// 而不是返回 Ok 把请求吞掉。
#[sqlx::test]
async fn test_settle_consumer_requeues_when_agent_busy(pool: SqlitePool) {
    let _ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("SettleQueueConflict-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let mgr = AgentRuntimeStateManager::global();
    mgr.set_busy(&agent_id, "msg-patrol", None, Some("proj-1"));

    let event = serde_json::to_value(AgentSettleEvent::new(
        &agent_id,
        5,
        "test agent rest trigger",
    ))
    .expect("event should serialize");

    let err = AgentSettleConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect_err("Agent 忙时应上抛冲突以触发队列重投");
    assert_eq!(
        err.code(),
        "conflict",
        "必须是可重试的冲突错误，而不是静默丢弃"
    );

    // 运行中的巡检上下文未被沉淀改写
    let info = mgr.get(&agent_id).unwrap();
    assert_eq!(info.current_message_id, Some("msg-patrol".to_string()));
    assert_eq!(info.project_id, Some("proj-1".to_string()));

    mgr.set_idle(&agent_id);
}

/// 抢占成功后必须释放：空闲 Agent 跑完一轮沉淀（此处无待沉淀记忆，正常空跑）
/// 要回到 Idle，否则它会永久卡在 Resting，之后所有消息只能 nack 重投。
#[sqlx::test]
async fn test_settle_consumer_releases_agent_after_run(pool: SqlitePool) {
    let _ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("SettleQueueRelease-{}", uuid::Uuid::now_v7()),
    )
    .await;

    let mgr = AgentRuntimeStateManager::global();
    mgr.set_idle(&agent_id);

    let event = serde_json::to_value(AgentSettleEvent::new(
        &agent_id,
        5,
        "test agent rest trigger",
    ))
    .expect("event should serialize");

    AgentSettleConsumer::new()
        .on_event(RequestContext::new_system(), event)
        .await
        .expect("无待沉淀记忆时应正常空跑，而不是报错");

    assert_eq!(
        mgr.get_state(&agent_id),
        common_ext::enums::AgentRuntimeState::Idle,
        "沉淀结束后 Agent 必须回到 Idle"
    );
}
