//! Integration tests for the queued sleep-settle path.
//!
//! # 回归场景
//! 日触发 `agent_rest` 与每小时项目巡检落在同一轮 cron poll，巡检先把**同一个** Agent
//! 唤醒成 Busy；旧实现在触发器里 `is_unavailable()` 判一下就 `Ok(0)` 静默跳过，而
//! 触发器随后把 `next_run_at` 推到下一个 cron 点（日触发 = 次日）→ 一次跳过丢一整天，
//! 界面上还显示「已执行」。
//!
//! # 现行机制（本文件锁定的契约）
//! 1. `CronTriggerConsumer`（`agent_rest`）**只派发** `agent.settle.requested` 到
//!    `agent.awakening` 队列，不碰 Agent 运行状态；Agent 忙也必须派发。
//! 2. `agent.settle.requested` 与 `message.created` 由**同一个**消费者消费（同队列、
//!    同 `order_key = agent_id`）—— 这是串行生效的前提：AOP 的 `order_key` 闸门
//!    按消费者隔离，拆成两个消费者时同一个 agent_id 会落在两条互不知晓的队列里。
//! 3. 消费者按封套 `kind` 分流，未订阅的 kind 显式报错，而不是被当成消息反序列化。
//! 4. 沉淀结束把 Agent 放回 Idle；抢不到（防御分支）上抛 `conflict` 而非静默跳过。
//! 5. `ack`/`nack` 按 `source` 分流：沉淀事件的 id 不是 message_id，不得去动 messages 表。
//!
//! 队列层「同 order_key 串行」这一不变量由 `src/pkg/aop/queue/in_memory.rs::tests`
//! 用实例隔离的单元测试锁定（不走全局 registry、不受并发测试干扰），
//! 这里只覆盖消费者与派发方的业务契约。

#[path = "../common/mod.rs"]
mod common;

extern crate common as common_ext;

use crate::common::TestApp;
use ai_orz::consumer::message::MessageConsumer;
use ai_orz::consumer::scheduler::CronTriggerConsumer;
use ai_orz::models::events::AgentSettleEvent;
use ai_orz::pkg::RequestContext;
use ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager;
use ai_orz::pkg::aop::Consumer;
use ai_orz::pkg::aop::queue::EventQueryFilter;
use serde_json::json;
use sqlx::SqlitePool;

/// 队列路由 key = 消费者名。两类事件共用它（这是串行的前提，见模块文档 2）
const AWAKENING_CONSUMER: &str = "agent.awakening";

const KIND_MESSAGE: &str = "message.created";
const KIND_SETTLE: &str = "agent.settle.requested";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 构造 `cron.trigger` 事件封套（`agent_rest` 动作，指定单个 Agent）
fn agent_rest_event(agent_id: &str) -> serde_json::Value {
    json!({
        "event_id": uuid::Uuid::now_v7().to_string(),
        "kind": "cron.trigger",
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

/// 构造 `agent.settle.requested` 事件封套
///
/// 框架 `publish` 会自动注入 `kind`；手工构造事件时必须自己带上 ——
/// 消费者正是靠它分流的。
fn settle_envelope(agent_id: &str, settle_limit: usize) -> serde_json::Value {
    let mut value: serde_json::Value = serde_json::to_value(AgentSettleEvent::new(
        agent_id,
        settle_limit,
        "test agent rest trigger",
    ))
    .expect("event should serialize");
    value["kind"] = json!(KIND_SETTLE);
    value
}

/// 统计某个 Agent 在「Agent 唤醒」队列里的待处理事件数（order_key = agent_id）
fn queued_for(agent_id: &str) -> usize {
    ai_orz::pkg::aop::registry()
        .query_events(
            AWAKENING_CONSUMER,
            EventQueryFilter {
                order_key: Some(agent_id.to_string()),
                ..Default::default()
            },
        )
        .expect("agent.awakening 队列应随消费者注册而创建")
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

    let before = queued_for(&agent_id);

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
        queued_for(&agent_id),
        before + 1,
        "Agent {} 忙也必须把沉淀请求排进队列",
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

/// 合并契约：消息与沉淀必须由**同一个**消费者（同一条队列）承接。
///
/// 拆开的话 `order_key = agent_id` 会在两条队列里各持一份闸门，串行保证静默失效。
#[sqlx::test]
async fn test_awakening_consumer_owns_both_kinds(pool: SqlitePool) {
    let _ctx = crate::common::init_full_test_env(pool.clone()).await;

    let consumer = MessageConsumer::new();
    assert_eq!(
        consumer.name(),
        AWAKENING_CONSUMER,
        "消费者名是队列路由 key，改动会让运行中队列与面板统计断档"
    );

    let subs = consumer.subscriptions();
    let kinds: Vec<&str> = subs.iter().map(|s| s.kind.as_str()).collect();
    assert!(
        kinds.contains(&KIND_MESSAGE),
        "Agent 唤醒消费者必须订阅 message.created，实际 {:?}",
        kinds
    );
    assert!(
        kinds.contains(&KIND_SETTLE),
        "沉淀必须与消息同属一个消费者，否则 order_key 串行跨消费者失效，实际 {:?}",
        kinds
    );
    // 两个订阅都必须声明 ordered：本消费者是全项目唯一 concurrency() > 1 的地方，
    // 也是唯一能观测 order_key 串行门闩的地方（漏声明 = 静默退回事故形态，不报错）。
    assert!(
        subs.iter().all(|s| s.ordered),
        "agent.awakening 的两个订阅都必须声明 ordered，实际 {:?}",
        subs
    );
}

/// 按 `kind` 分流：未订阅的事件类型必须显式报错
///
/// 否则它会被当成 `MessageCreatedEvent` 反序列化（字段碰巧同名时会静默产生错误行为）。
#[sqlx::test]
async fn test_awakening_consumer_rejects_unsubscribed_kind(pool: SqlitePool) {
    let _ctx = crate::common::init_full_test_env(pool.clone()).await;

    let err = MessageConsumer::new()
        .on_event(
            RequestContext::new_system(),
            json!({"kind": "task.status_changed", "event_id": "evt-unknown"}),
        )
        .await
        .expect_err("未订阅的事件类型必须显式报错");

    assert!(
        err.to_string().contains("task.status_changed"),
        "错误信息应指出真实的 kind，实际：{}",
        err
    );
}

/// 防御性不变量：抢不到 Agent 时必须上报冲突（不静默跳过）
///
/// 正常路径不可达（同队列同 order_key + 本消费者是唯一把 Agent 置忙的链路），
/// 这里直接构造「Agent 已忙」来验证抢不到时的行为。
#[sqlx::test]
async fn test_awakening_consumer_reports_conflict_when_agent_busy(pool: SqlitePool) {
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

    let err = MessageConsumer::new()
        .on_event(RequestContext::new_system(), settle_envelope(&agent_id, 5))
        .await
        .expect_err("Agent 忙时应上抛冲突以触发队列重投，而不是静默跳过");

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
/// 要回到 Idle，否则它会永久卡在 Resting，之后所有消息只能被 nack 重投。
#[sqlx::test]
async fn test_awakening_consumer_settles_and_releases_agent(pool: SqlitePool) {
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

    MessageConsumer::new()
        .on_event(RequestContext::new_system(), settle_envelope(&agent_id, 5))
        .await
        .expect("无待沉淀记忆时应正常空跑，而不是报错");

    assert_eq!(
        mgr.get_state(&agent_id),
        common_ext::enums::AgentRuntimeState::Idle,
        "沉淀结束后 Agent 必须回到 Idle"
    );
}

/// `ack`/`nack` 必须按 `source` 分流：沉淀事件的 event_id **不是** message_id
///
/// 不设这道闸门的话，每个沉淀事件都会对 messages 表白跑一次
/// `UPDATE ... WHERE id = <settle_event_id>`（命中 0 行、静默 Ok），把「传进来的 id
/// 一定指向 messages 表」变成一个没说出口的前提 —— 将来给 `update_status` 加上行数
/// 校验，沉淀链路就会开始 nack → 重投 → 再失败，变成死循环。
#[sqlx::test]
async fn test_ack_by_source_does_not_touch_messages(pool: SqlitePool) {
    let _ctx = crate::common::init_full_test_env(pool.clone()).await;

    // 造一条真实消息行（Pending），并用它的 id 冒充「沉淀事件 id」
    let message_id = format!("msg-{}", uuid::Uuid::now_v7());
    let now = now_ms();
    let pending = common_ext::enums::MessageStatus::Pending as i32;
    sqlx::query(
        "INSERT INTO messages (id, from_id, to_id, from_role, to_role, message_type, status, \
         content, created_by, modified_by, created_at, updated_at) \
         VALUES (?, 'user-1', 'agent-1', ?, ?, 1, ?, 'hi', 'test', 'test', ?, ?)",
    )
    .bind(&message_id)
    .bind(common_ext::enums::MessageRole::User as i32)
    .bind(common_ext::enums::MessageRole::Agent as i32)
    .bind(pending)
    .bind(now)
    .bind(now)
    .execute(ai_orz::pkg::storage::get().sqlite_pool())
    .await
    .expect("应能插入测试消息行");

    let consumer = MessageConsumer::new();

    // ① source = 沉淀事件：不得借这个 id 去改 messages 表
    consumer
        .ack(KIND_SETTLE, &message_id)
        .await
        .expect("非消息事件的 ack 应为无操作");
    consumer
        .nack(KIND_SETTLE, &message_id)
        .await
        .expect("非消息事件的 nack 应为无操作");

    let status = read_message_status(&message_id).await;
    assert_eq!(
        status, pending,
        "沉淀事件的 ack/nack 不该改动 messages 表，实际 status={}",
        status
    );

    // ② source = message.created：正常对账，状态真的要变
    consumer
        .ack(KIND_MESSAGE, &message_id)
        .await
        .expect("消息事件的 ack 应正常落库");

    let status = read_message_status(&message_id).await;
    assert_eq!(
        status,
        common_ext::enums::MessageStatus::Processed as i32,
        "message.created 的 ack 必须把消息置为 Processed"
    );
}

async fn read_message_status(message_id: &str) -> i32 {
    sqlx::query_scalar::<_, i32>("SELECT status FROM messages WHERE id = ?")
        .bind(message_id)
        .fetch_one(ai_orz::pkg::storage::get().sqlite_pool())
        .await
        .expect("消息行应仍存在")
}
