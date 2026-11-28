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

/// 长退避队列（催促语义用例专用）：避免测试执行慢于退避窗口导致假通过
async fn new_queue_with_long_backoff() -> InMemoryEventQueue {
    crate::pkg::storage::test_support::init_for_test().await;
    InMemoryEventQueue::with_backoff_policy(60_000, 60_000)
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

/// 退避窗口护栏：8 次尝试加起来的静默时长必须落在「Agent 忙完一轮」的尺度内
///
/// 背景（2026-10-18）：`base=1s / max=60s` 时 8 次尝试约 2 分钟就走完 —— **短于
/// Agent 处理一条消息的典型耗时**。于是「Agent 正忙」导致的 `[conflict]` 会在它
/// 忙完之前把 8 次机会全部耗尽 → `Discard` → 消息永久丢失（且丢的是「任务完成
/// 通知」这类没有第二次补发机会的消息）。
///
/// 这里把完整窗口钉在 15 ~ 20 分钟：下限保证等得到 Agent 空闲，上限保证不会
/// 把队列（以及同 `order_key` 门闩）占死到天荒地老。
#[test]
fn retry_backoff_window_covers_agent_busy_round() {
    use crate::pkg::aop::DEFAULT_MAX_ATTEMPTS;

    let mut total_ms = 0i64;
    for attempt in 2..=DEFAULT_MAX_ATTEMPTS {
        total_ms += super::retry_backoff_ms(
            super::RETRY_BACKOFF_BASE_MS,
            super::RETRY_BACKOFF_MAX_MS,
            attempt,
        );
    }

    assert!(
        total_ms >= 15 * 60_000,
        "完整退避窗口 {}s 太短 —— 撑不到 Agent 忙完一轮，8 次机会会白烧掉",
        total_ms / 1_000
    );
    assert!(
        total_ms <= 20 * 60_000,
        "完整退避窗口 {}s 太长 —— 一条坏事件会占住队列与 order_key 门闩过久",
        total_ms / 1_000
    );
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

/// 同 key 新消息 = 用户催促：nack 退避中的队首事件立即恢复可取，
/// 但 attempt 不重置（重试计数不因催促丢失）
#[tokio::test]
async fn same_key_enqueue_clears_backoff_and_keeps_attempt() {
    let queue = new_queue_with_long_backoff().await;
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e1", "message.created", "agent-a", 1),
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

    assert!(
        queue
            .dequeue_next(RequestContext::new_system())
            .await
            .unwrap()
            .is_none(),
        "催促前仍在静默期"
    );

    // 用户补发一条同 agent 消息 = 催促
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e2", "message.created", "agent-a", 2),
        )
        .await
        .unwrap();

    let retried = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .expect("催促后 e1 应立即可取");
    assert_eq!(retried["event_id"], "e1");
    assert_eq!(
        retried["attempt"].as_u64(),
        Some(2),
        "attempt 不因催促而重置"
    );
}

/// 催促不得波及其他 key 的退避（全堆遍历最常见的写错点）
#[tokio::test]
async fn nudge_does_not_clear_backoff_of_other_order_keys() {
    let queue = new_queue_with_long_backoff().await;
    let ctx = RequestContext::new_system();

    for (id, key, at) in [("a1", "agent-a", 1), ("b1", "agent-b", 2)] {
        queue
            .enqueue(ctx.clone(), envelope(id, "message.created", key, at))
            .await
            .unwrap();
    }
    // 先让 b1 进 in_progress，再 nack 进退避；a1 还没被 dequeue，仍在 global（无退避）
    let first = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(first["event_id"], "a1");
    queue.ack(ctx.clone(), "a1").await.unwrap();
    let second = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(second["event_id"], "b1");
    queue.nack(ctx.clone(), "b1").await.unwrap();
    let _ = first; // 仅用于断言 a1 先出，后续不再使用

    // a2 催促 agent-a：b1 属于 agent-b，不得被清退避
    queue
        .enqueue(ctx.clone(), envelope("a2", "message.created", "agent-a", 3))
        .await
        .unwrap();
    let got = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(got["event_id"], "a2", "同 key 催促事件正常出队");
    assert!(
        queue.dequeue_next(ctx).await.unwrap().is_none(),
        "agent-b 的 b1 仍在退避中"
    );
}

/// 多次催促幂等：同一退避事件只被清一次，attempt 不叠加
#[tokio::test]
async fn repeated_nudges_are_idempotent_for_attempt() {
    let queue = new_queue_with_long_backoff().await;
    let ctx = RequestContext::new_system();
    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "agent-a", 1))
        .await
        .unwrap();
    let first = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(first["event_id"], "e1");
    queue.nack(ctx.clone(), "e1").await.unwrap();

    for i in 2..=4 {
        queue
            .enqueue(
                ctx.clone(),
                envelope(&format!("e{i}"), "message.created", "agent-a", i),
            )
            .await
            .unwrap();
    }
    let retried = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(retried["event_id"], "e1");
    assert_eq!(
        retried["attempt"].as_u64(),
        Some(2),
        "多次催促不叠加 attempt"
    );
}

/// 已到期的退避事件不应被「清退避」误改：本就可取，无需干预
#[tokio::test]
async fn nudge_does_not_touch_already_due_event() {
    crate::pkg::storage::test_support::init_for_test().await;
    let queue = InMemoryEventQueue::with_backoff_policy(0, 0);
    let ctx = RequestContext::new_system();
    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "agent-a", 1))
        .await
        .unwrap();
    let first = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(first["event_id"], "e1", "首次出队应是 e1");
    assert_eq!(first["attempt"].as_u64(), Some(1), "首次消费 attempt=1");
    queue.nack(ctx.clone(), "e1").await.unwrap();
    // 退避为 0，e1 本就立即取得到；此时同 key 新消息入队不应改变 attempt
    queue
        .enqueue(ctx.clone(), envelope("e2", "message.created", "agent-a", 2))
        .await
        .unwrap();
    let got = queue.dequeue_next(ctx).await.unwrap().unwrap();
    assert_eq!(got["event_id"], "e1");
    assert_eq!(got["attempt"].as_u64(), Some(2));
}

/// 催促只清退避、不放行门闩：同 key 后继仍要等前一条 ack
#[tokio::test]
async fn nudge_keeps_order_key_gate_and_unlocks_on_ack() {
    let queue = new_queue_with_long_backoff().await;
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e1", "message.created", "agent-a", 1),
        )
        .await
        .unwrap();
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e2", "message.created", "agent-a", 2),
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

    // e3 入队触发催促，e1 恢复可取，但它仍是同 key 独占者
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e3", "message.created", "agent-a", 3),
        )
        .await
        .unwrap();
    let retried = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retried["event_id"], "e1", "催促不得绕过门闩");
    assert!(
        queue
            .dequeue_next(RequestContext::new_system())
            .await
            .unwrap()
            .is_none(),
        "e1 未 ack 前，e2/e3 不得出队"
    );

    queue.ack(RequestContext::new_system(), "e1").await.unwrap();
    let successor = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(successor["event_id"], "e2", "ack 后门闩按 FIFO 放行");
}

/// ungated 事件不能触发 gated 退避清零：门闩是订阅者 opt-in，旁路不得影响
#[tokio::test]
async fn ungated_same_key_enqueue_does_not_clear_gated_backoff() {
    let queue = new_queue_with_long_backoff().await;
    queue
        .enqueue(
            RequestContext::new_system(),
            envelope("e1", "message.created", "agent-a", 1),
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

    queue
        .enqueue_ungated(
            RequestContext::new_system(),
            envelope("e2", "stats.collected", "agent-a", 2),
        )
        .await
        .unwrap();

    // ungated 事件可立即出队；若误清了 e1 退避，堆顶会先出 e1（created_at 更早）
    let second = queue
        .dequeue_next(RequestContext::new_system())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second["event_id"], "e2");
    assert!(
        queue
            .dequeue_next(RequestContext::new_system())
            .await
            .unwrap()
            .is_none(),
        "gated 事件 e1 应仍在退避中"
    );
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

/// 回归：含中文（多字节）payload 的事件详情**完整返回、不截断、不 panic**
///
/// 历史：`get_event` 曾按前 200 字节截断，第 200 字节落在汉字（3 字节）中间时
/// panic（`byte index N is not a char boundary`）——中文消息必踩，且该 panic 发生在
/// **持队列锁**期间 → std Mutex 永久 poison → 消费者队列永久不可用。现改为完整返回。
#[tokio::test]
async fn get_event_returns_full_cjk_payload_without_truncation() {
    let queue = new_queue().await;

    // 足以让序列化结果远超 200 字节的中文正文
    let long_text = "汉字测试".repeat(60);
    let mut ev = envelope("cn1", "message.created", "", 1);
    ev["text"] = serde_json::json!(long_text);
    queue
        .enqueue(RequestContext::new_system(), ev)
        .await
        .unwrap();

    let detail = queue.get_event("cn1").expect("事件详情应可取到");
    // 完整 payload：含完整中文正文，且无截断标记
    assert!(
        detail.payload.contains(&long_text),
        "payload 应为完整内容，实际：{}",
        detail.payload
    );
    assert!(
        !detail.payload.contains("truncated"),
        "不应再出现截断标记，实际：{}",
        detail.payload
    );
    assert!(detail.payload.len() > 200, "完整 payload 应超过 200 字节");
}

/// 回归：队列锁被 poison 后仍能自愈继续工作
///
/// 背景：`get_event` 的字节切片 panic 发生在**持队列锁**期间 → std `Mutex` 永久
/// poison，之后 awakening 侧 `dequeue error: ... poisoned lock: another task failed
/// inside` 死循环 → 用户侧「发消息没响应」。本测试直接模拟「持锁 panic」，断言
/// `lock_guard` 恢复后 enqueue / dequeue 依旧正常。
#[tokio::test]
async fn queue_recovers_after_lock_poisoned() {
    crate::pkg::storage::test_support::init_for_test().await;
    let queue = std::sync::Arc::new(InMemoryEventQueue::new());

    // 另一线程持锁并 panic → Mutex 被 poison
    let q2 = std::sync::Arc::clone(&queue);
    let join = std::thread::spawn(move || {
        let _guard = q2.lock_guard();
        panic!("intentional panic while holding queue lock");
    })
    .join();
    assert!(join.is_err(), "子线程应因 panic 失败（锁已 poison）");

    // 锁虽已 poison，但 lock_guard 恢复 → 队列照常工作
    let ctx = RequestContext::new_system();
    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "", 1))
        .await
        .expect("poison 后 enqueue 仍应成功");
    let got = queue
        .dequeue_next(ctx)
        .await
        .expect("poison 后 dequeue 仍应成功")
        .expect("事件应可取到");
    assert_eq!(got["event_id"], "e1");
}

/// 催促时效性缺口：旧事件仍在 `in_progress` 时同 key 新消息入队，清退避够不到它；
/// 这次催促应由时间戳接力到 `nack`，让失败重投立即可取。
#[tokio::test]
async fn nudge_during_in_progress_skips_backoff_on_nack() {
    let queue = new_queue_with_long_backoff().await;
    let ctx = RequestContext::new_system();

    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "agent-a", 1))
        .await
        .unwrap();
    let first = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(first["event_id"], "e1");

    // 保证催促时刻严格晚于本次消费开始时刻（毫秒时钟精度）
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // e1 仍在 in_progress，不在 global 堆里 —— 清退避路径够不到它
    queue
        .enqueue(ctx.clone(), envelope("e2", "message.created", "agent-a", 2))
        .await
        .unwrap();

    queue.nack(ctx.clone(), "e1").await.unwrap();
    let retried = queue
        .dequeue_next(ctx.clone())
        .await
        .unwrap()
        .expect("in_progress 期间的催促应在 nack 后立即可取");
    assert_eq!(retried["event_id"], "e1");
    assert_eq!(retried["attempt"].as_u64(), Some(2), "催促不重置 attempt");

    // 一次性：再次 nack 没有新催促 → 恢复指数退避，不成紧循环
    queue.nack(ctx.clone(), "e1").await.unwrap();
    assert!(
        queue.dequeue_next(ctx).await.unwrap().is_none(),
        "一次催促只兑现一次，之后应恢复退避"
    );
}

/// 已在退避中的事件被催促：入队时清退避已兑现一次；随后重新消费时
/// `consumed_at` 晚于催促时刻，再次 nack 不得二次免退避。
#[tokio::test]
async fn nudge_while_already_in_backoff_is_not_double_exempted() {
    let queue = new_queue_with_long_backoff().await;
    let ctx = RequestContext::new_system();

    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "agent-a", 1))
        .await
        .unwrap();
    queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    queue.nack(ctx.clone(), "e1").await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    queue
        .enqueue(ctx.clone(), envelope("e2", "message.created", "agent-a", 2))
        .await
        .unwrap();

    let retried = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(retried["event_id"], "e1", "催促后队首立即恢复可取");
    queue.nack(ctx.clone(), "e1").await.unwrap();
    assert!(
        queue.dequeue_next(ctx).await.unwrap().is_none(),
        "入队时的清退避已兑现催促，不得在 nack 再免一次"
    );
}

/// 催促按 order_key 隔离：别的 key 来消息不得让本 key 的 nack 免退避
#[tokio::test]
async fn unrelated_key_nudge_does_not_exempt_in_progress_nack() {
    let queue = new_queue_with_long_backoff().await;
    let ctx = RequestContext::new_system();

    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "agent-a", 1))
        .await
        .unwrap();
    queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    queue
        .enqueue(ctx.clone(), envelope("e2", "message.created", "agent-b", 2))
        .await
        .unwrap();
    queue.nack(ctx.clone(), "e1").await.unwrap();

    // 不同 key 互不阻塞：e2 立即可取，e1 仍应正常退避
    let got = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(got["event_id"], "e2");
    assert!(
        queue.dequeue_next(ctx).await.unwrap().is_none(),
        "无关 key 的催促不得免 e1 的退避"
    );
}

/// ungated 同 key 入队不算催促：门闩是订阅者 opt-in，旁路事件不得触发 gated 免退避
#[tokio::test]
async fn ungated_same_key_enqueue_does_not_exempt_gated_nack() {
    let queue = new_queue_with_long_backoff().await;
    let ctx = RequestContext::new_system();

    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "agent-a", 1))
        .await
        .unwrap();
    queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    queue
        .enqueue_ungated(ctx.clone(), envelope("u1", "stats.collected", "agent-a", 2))
        .await
        .unwrap();
    queue.nack(ctx.clone(), "e1").await.unwrap();

    let got = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(got["event_id"], "u1", "ungated 事件绕开门闩先出");
    assert!(
        queue.dequeue_next(ctx).await.unwrap().is_none(),
        "ungated 同 key 不得触发 gated 事件免退避"
    );
}

/// ack 后门闩状态变化，此前指向旧事件的催促标记必须清除，不能误免后继的退避
#[tokio::test]
async fn ack_clears_stale_nudge_marker() {
    let queue = new_queue_with_long_backoff().await;
    let ctx = RequestContext::new_system();

    queue
        .enqueue(ctx.clone(), envelope("e1", "message.created", "agent-a", 1))
        .await
        .unwrap();
    queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    queue
        .enqueue(ctx.clone(), envelope("e2", "message.created", "agent-a", 2))
        .await
        .unwrap();

    queue.ack(ctx.clone(), "e1").await.unwrap();
    let second = queue.dequeue_next(ctx.clone()).await.unwrap().unwrap();
    assert_eq!(second["event_id"], "e2");

    queue.nack(ctx.clone(), "e2").await.unwrap();
    assert!(
        queue.dequeue_next(ctx).await.unwrap().is_none(),
        "旧催促指向 e1，ack 后不得让 e2 免退避"
    );
}
