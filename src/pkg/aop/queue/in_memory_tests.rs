//! tests 单元测试（拆分自 in_memory.rs）
//!
//! 文件瘦身：原 1048 行 → 566 行，测试体 483 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use serde_json::json;

/// 构造最小事件封套（队列只读元字段，业务字段与队列无关）
fn envelope(event_id: &str, kind: &str, order_key: &str, created_at: i64) -> serde_json::Value {
    json!({
        "event_id": event_id,
        "kind": kind,
        "order_key": order_key,
        "priority": 0,
        "created_at": created_at,
    })
}

/// 每个测试用**自己**的队列实例（不走全局 registry），互不干扰
async fn new_queue() -> InMemoryEventQueue {
    crate::pkg::storage::test_support::init_for_test().await;
    InMemoryEventQueue::new()
}

/// 同一 `order_key` 必须严格串行：前一条 ack 之前后继**不可出队**。
///
/// 这是「同一 Agent 的沉淀与消息串行」的支点 —— 契约要求串行点落在**队列层**，
/// 而不是让业务层去抢占失败、靠 nack 重投兜底（那会把日志与失败指标刷爆）。
/// 回归场景：`message.created`（to_role = Agent 时 order_key = agent_id）与
/// `agent.settle.requested`（order_key = agent_id）落在同一条串行链上。
#[tokio::test]
async fn same_order_key_serializes_until_ack() {
    let queue = new_queue().await;

    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e1", "message.created", "agent-1", 1),
        )
        .await
        .unwrap();
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e2", "agent.settle.requested", "agent-1", 2),
        )
        .await
        .unwrap();

    let first = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .expect("队首应可出队");
    assert_eq!(first["event_id"], "e1");

    assert!(
        queue
            .dequeue_next(RequestContext::new_system())
            .await
            .unwrap()
            .is_none(),
        "同 order_key 的后继必须等前一条 ack —— 串行点必须在队列层，不能靠抢占失败重试"
    );

    queue.ack(RequestContext::new_system(), "e1").await.unwrap();
    let second = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .expect("ack 后后继应可出队");
    assert_eq!(second["event_id"], "e2");

    assert!(
        queue
            .dequeue_next(RequestContext::new_system())
            .await
            .unwrap()
            .is_none()
    );
}

/// 护栏：ack 放行「最后一个后继」后，门闩**仍被该后继占用**
///
/// 回归来源：曾在 ack 里先 `has_active_message.insert(key, true)`（后继上堆），
/// 紧接着又被 `if queue.is_empty()` 分支 `remove` 掉 —— 队列空了但后继还在堆里。
/// 于是下一个同 key 事件判定「门闩空闲」直接上堆 → 同 key 两事件并存在堆里
/// → 并发 > 1 时被并行消费，串行门闩静默失效（且 FIFO 可能乱序）。
#[tokio::test]
async fn latch_stays_held_when_ack_releases_last_successor() {
    let queue = new_queue().await;
    let ctx = RequestContext::new_system();

    for (id, created_at) in [("e1", 1), ("e2", 2)] {
        queue
            .enqueue(
                ctx.clone(),
                envelope(id, "message.created", "agent-1", created_at),
            )
            .await
            .unwrap();
    }

    let first = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(first["event_id"], "e1");
    // e1 结束 → e2 上堆（此时 key 队列已空，正是当初踩空的形状）
    queue.ack(ctx.clone(), "e1").await.unwrap();

    // e3 入队：e2 还在堆里未被消费 → e3 必须继续排在门闩后面
    queue
        .enqueue(ctx.clone(), envelope("e3", "message.created", "agent-1", 3))
        .await
        .unwrap();

    let second = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(second["event_id"], "e2", "e2 应先出队");

    let third = queue.dequeue_next(ctx.clone()).await.unwrap();
    assert!(
        third.is_none(),
        "e2 未 ack 前 e3 不应出队（门闩应仍被 e2 占用），实际出队：{:?}",
        third.map(|v| v["event_id"].clone())
    );
}

/// 混用陷阱 ①：同一消费者里，ungated 事件的 `ack` **不得**放行 gated 队列的后继
///
/// 一个消费者只有**一个**队列，而 `order_key` 是**事件侧**的值 —— 同一消费者下
/// 「A 订阅声明 ordered、B 订阅没声明」而两者 key 都是 `agent-1` 时，就会一条进闸、
/// 一条绕闸。若 `ack` 不区分来路，只看 key 去 pop 后继，B 的 ack 会把 A 的串行保证
/// 悄悄吃掉（并发 > 1 时同 key 被并行消费）。
#[tokio::test]
async fn ungated_ack_does_not_release_gated_successor() {
    let queue = new_queue().await;
    let ctx = RequestContext::new_system();

    queue
        .enqueue(ctx.clone(), envelope("g1", "message.created", "agent-1", 1))
        .await
        .unwrap();
    let first = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(first["event_id"], "g1");

    // gated 后继排在门闩后面
    queue
        .enqueue(ctx.clone(), envelope("g2", "message.created", "agent-1", 2))
        .await
        .unwrap();
    // 同 key 的 ungated 事件：绕开门闩直进堆，先于 g2 被消费
    queue
        .enqueue_ungated(ctx.clone(), envelope("u1", "agent_loop", "agent-1", 3))
        .await
        .unwrap();

    let raced = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(raced["event_id"], "u1", "ungated 事件应当绕开门闩先出队");

    // u1 成功 → 它的 ack 不得触碰 g2 所在的门闩队列
    queue.ack(ctx.clone(), "u1").await.unwrap();
    let leaked = queue.dequeue_next(ctx.clone()).await.unwrap();
    assert!(
        leaked.is_none(),
        "g1 尚未 ack，g2 不得因 ungated 事件的 ack 被放行，实际出队：{:?}",
        leaked.map(|v| v["event_id"].clone())
    );
}

/// 混用陷阱 ②：ungated 事件的 `nack` **不得**给该 key 打上门闩占用
///
/// 反过来的形状：重投 Ungated 事件时若也 `has_active_message[key] = true`，
/// 而它自己不是从 key 队列出来的 —— 没有哪个 gated 的 ack 会为它收尾，
/// 该标记永远解不开 → 同 key 的 gated 事件永久饥饿。
#[tokio::test]
async fn ungated_nack_does_not_hold_the_latch() {
    let queue = new_queue().await;
    let ctx = RequestContext::new_system();

    queue
        .enqueue_ungated(ctx.clone(), envelope("u1", "agent_loop", "agent-9", 1))
        .await
        .unwrap();
    let first = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(first["event_id"], "u1");
    queue.nack(ctx.clone(), "u1").await.unwrap();

    // 同 key 的 gated 事件：**必须**能立刻进堆（门闩未被 u1 占用）
    queue
        .enqueue(ctx.clone(), envelope("g1", "message.created", "agent-9", 2))
        .await
        .unwrap();
    let gated = queue
        .dequeue_next(ctx.clone())
        .await
        .unwrap()
        .expect("ungated 事件的重投不得占住同 key 的门闩");
    assert_eq!(gated["event_id"], "g1");
}

/// 不同 `order_key` 互不阻塞：一个慢 Agent 不该拖住整条队列
#[tokio::test]
async fn different_order_keys_do_not_block_each_other() {
    let queue = new_queue().await;

    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e1", "message.created", "agent-1", 1),
        )
        .await
        .unwrap();
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e2", "message.created", "agent-2", 2),
        )
        .await
        .unwrap();

    let mut ids = Vec::new();
    while let Some(event) = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
    {
        ids.push(event["event_id"].as_str().unwrap().to_string());
    }
    ids.sort();
    assert_eq!(ids, vec!["e1", "e2"]);
}

/// `attempt` = 本事件被消费的**累计次数**：入队即 1，每次 `nack` 自增
///
/// 它是 `Producer::on_failed` 实现「退避 N 次后放弃」的唯一依据 ——
/// 没有它，生产者只能凭错误内容猜，无法表达「重试太多次了，别再试了」。
#[tokio::test]
async fn attempt_increments_on_each_nack() {
    let queue = new_queue_with_fast_backoff().await;
    let ctx = RequestContext::new_system();

    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "", 1))
        .await
        .unwrap();

    let first = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(
        first["attempt"].as_u64(),
        Some(1),
        "首次消费即 attempt == 1"
    );

    queue.nack(ctx.clone(), "e1").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(
        (FAST_BACKOFF_MAX_MS + 30) as u64,
    ))
    .await;
    let second = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(second["attempt"].as_u64(), Some(2));

    queue.nack(ctx.clone(), "e1").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(
        (FAST_BACKOFF_MAX_MS + 30) as u64,
    ))
    .await;
    let third = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(
        third["attempt"].as_u64(),
        Some(3),
        "连续 nack 两次后第 3 次取出应为 attempt == 3"
    );
}

/// `ack` 后事件从队列消失、计数随之消亡（**无需持久化**）；同 id 重新入队从 1 重算
#[tokio::test]
async fn attempt_resets_when_event_is_reenqueued() {
    // 毫秒级退避：nack 重投后需等退避窗口过去才能再次取出
    let queue = new_queue_with_fast_backoff().await;
    let ctx = RequestContext::new_system();

    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "", 1))
        .await
        .unwrap();
    let _ = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    queue.nack(ctx.clone(), "e1").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(
        (FAST_BACKOFF_MAX_MS + 30) as u64,
    ))
    .await;

    let _ = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    queue.ack(ctx.clone(), "e1").await.unwrap();

    // 同一个 id 重新入队 = 新的生命周期 → 计数回到 1
    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "", 1))
        .await
        .unwrap();
    let again = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(again["attempt"].as_u64(), Some(1));
}

/// nack 把事件放回可调度堆（重试而非丢弃），且不会把同 `order_key` 的后继锁死
#[tokio::test]
async fn nack_requeues_event_and_keeps_successor_blocked() {
    let queue = new_queue_with_fast_backoff().await;

    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e1", "message.created", "agent-1", 1),
        )
        .await
        .unwrap();
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e2", "message.created", "agent-1", 2),
        )
        .await
        .unwrap();

    let first = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first["event_id"], "e1");

    queue
        .nack(RequestContext::new_system(), "e1")
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(
        (FAST_BACKOFF_MAX_MS + 30) as u64,
    ))
    .await;

    // 失败事件自己重投（后继仍被挡住）
    let retried = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .expect("nack 后事件应重新可调度");
    assert_eq!(retried["event_id"], "e1");

    // ack 之后才轮到后继
    queue.ack(RequestContext::new_system(), "e1").await.unwrap();
    let next = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .expect("ack 后后继应可出队");
    assert_eq!(next["event_id"], "e2");
}

const FAST_BACKOFF_BASE_MS: i64 = 10;
const FAST_BACKOFF_MAX_MS: i64 = 50;

/// 毫秒级退避队列（退避用例专用；`new_queue()` 保持生产参数）
async fn new_queue_with_fast_backoff() -> InMemoryEventQueue {
    crate::pkg::storage::test_support::init_for_test().await;
    InMemoryEventQueue::with_backoff_policy(FAST_BACKOFF_BASE_MS, FAST_BACKOFF_MAX_MS)
}

/// 退避曲线：按 attempt 指数递增并封顶
#[test]
fn retry_backoff_grows_exponentially_and_caps() {
    let base = 1_000;
    let max = 60_000;
    assert_eq!(
        super::retry_backoff_ms(base, max, 1),
        base,
        "attempt=1 不会发生（重投前已自增），取 base 兜底"
    );
    assert_eq!(super::retry_backoff_ms(base, max, 2), 1_000);
    assert_eq!(super::retry_backoff_ms(base, max, 3), 2_000);
    assert_eq!(super::retry_backoff_ms(base, max, 4), 4_000);
    assert_eq!(super::retry_backoff_ms(base, max, 8), 60_000.min(base << 6));
    assert_eq!(super::retry_backoff_ms(base, max, 100), max, "封顶");
}

/// nack 后事件进入退避静默期：未到期不可出队，但**不阻塞**堆里已到期的其他事件
#[tokio::test]
async fn nack_backoff_defers_event_without_blocking_due_ones() {
    let queue = new_queue_with_fast_backoff().await;

    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e1", "message.created", "", 1),
        )
        .await
        .unwrap();
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e2", "message.created", "", 2),
        )
        .await
        .unwrap();

    let first = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first["event_id"], "e1");
    queue
        .nack(RequestContext::new_system(), "e1")
        .await
        .unwrap();

    // 退避静默期内：e1 取不到，但 e2（已到期）不被阻塞
    let due = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .expect("退避中的 e1 不得阻塞已到期的 e2");
    assert_eq!(due["event_id"], "e2");
    queue.ack(RequestContext::new_system(), "e2").await.unwrap();

    assert!(
        queue
            .dequeue_next(RequestContext::new_system())
            .await
            .unwrap()
            .is_none(),
        "静默期内 e1 不可出队"
    );

    // 退避窗口过后：e1 回来，且 attempt 已自增
    tokio::time::sleep(std::time::Duration::from_millis(
        (FAST_BACKOFF_MAX_MS + 30) as u64,
    ))
    .await;
    let retried = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .expect("退避结束后 e1 应可出队");
    assert_eq!(retried["event_id"], "e1");
    assert_eq!(retried["attempt"].as_u64(), Some(2));
}

/// §4.1 wiring：未声明 `ordered` 的订阅走 `enqueue_ungated`——
/// 事件即使带 `order_key` 也不进同 key 门闩队列，立即可出队
#[tokio::test]
async fn ungated_enqueue_bypasses_order_key_gate() {
    let queue = new_queue().await;

    queue
        .enqueue_ungated(
            RequestContext::new_system(),
            envelope("e1", "stats.collected", "agent-1", 1),
        )
        .await
        .unwrap();
    // 同 key 第二条：门闩队列会把它锁到 e1 ack 之后；ungated 必须立即可取
    queue
        .enqueue_ungated(
            RequestContext::new_system(),
            envelope("e2", "stats.collected", "agent-1", 2),
        )
        .await
        .unwrap();

    let first = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .expect("ungated 事件应立即可出队");
    assert_eq!(first["event_id"], "e1");

    let second = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .expect("ungated 同 key 后继不受门闩约束");
    assert_eq!(second["event_id"], "e2");

    // 未进 key 队列 → order_key 统计应为空
    assert!(
        queue.stats().order_keys.is_empty(),
        "ungated 事件不得出现在 order_key 门闩统计里"
    );
}
