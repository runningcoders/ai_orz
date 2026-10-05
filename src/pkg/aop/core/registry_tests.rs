//! tests 单元测试（拆分自 registry.rs）
//!
//! 文件瘦身：原 1531 行 → 884 行，测试体 648 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::pkg::aop::{EventSink, ProducerLoop, Subscription};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicUsize;

#[derive(Clone, Serialize, Deserialize)]
struct ShutdownTestEvent {
    id: String,
}

impl Event for ShutdownTestEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::AgentStateChanged
    }
    fn id(&self) -> &str {
        &self.id
    }
}

struct CountingConsumer {
    consumed: Arc<AtomicUsize>,
}

#[async_trait]
impl Consumer for CountingConsumer {
    fn name(&self) -> &str {
        "shutdown_test_consumer"
    }
    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::new(EventTopic::AgentStateChanged)]
    }
    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }
    fn empty_queue_sleep_ms(&self) -> u64 {
        20
    }
    async fn on_event(&self, _ctx: RequestContext, _event: serde_json::Value) -> Result<()> {
        self.consumed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// 生命周期契约的最小生产者：start 记录调用、stop 置位
///
/// `topic()` 选一个本模块测试独占的 topic（不与其它测试桩冲突）。
struct StoppableProducer {
    started: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
}

#[async_trait]
impl Producer for StoppableProducer {
    fn name(&self) -> &str {
        "shutdown_test_producer"
    }
    fn topic(&self) -> EventTopic {
        EventTopic::OrganizationChanged
    }
    /// 契约 1：spawn 后立即返回。这里没有 loop，直接返回即可。
    async fn start(&self, _sink: EventSink) -> Result<()> {
        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn stop(&self) -> Result<()> {
        self.stopped.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// 待回调的 topic（与 `ProbeConsumer` 的订阅、`ProbeProducer` 的归属一致）
#[derive(Clone, Serialize, Deserialize)]
struct ProbeEvent {
    id: String,
}

impl Event for ProbeEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::TaskStatusChanged
    }
    fn id(&self) -> &str {
        &self.id
    }
}

/// 声明了 `notify_producer` 的 Async 消费者
///
/// - `fail`：控制 `on_event` 成败；
/// - `decision`：失败后的终局判定 —— **消费者说了算**（这是 2026-09-17 判定权
///   下沉后的新契约，生产者只接收结论）。
struct ProbeConsumer {
    fail: bool,
    decision: RetryDecision,
}

#[async_trait]
impl Consumer for ProbeConsumer {
    fn name(&self) -> &str {
        "probe_consumer"
    }
    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::new(EventTopic::TaskStatusChanged).notify_producer()]
    }
    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }
    fn empty_queue_sleep_ms(&self) -> u64 {
        10
    }
    fn decide_retry(&self, _err: &str, _attempt: u32) -> RetryDecision {
        self.decision
    }
    async fn on_event(&self, _ctx: RequestContext, _event: serde_json::Value) -> Result<()> {
        if self.fail {
            return Err(err!(Internal, "probe consumer intentional failure"));
        }
        Ok(())
    }
}

/// 记录回调事实的生产者：既可验证 ack/重投/Discard，也可验证「回调先于 queue.ack」
///
/// `sees_decision` 记录 `on_failed` **收到的**结论 —— 它是消费者给的，不是生产者答的。
struct ProbeProducer {
    /// `on_failed` 自身是否直接报错（验证「收尾失败不改投递结论」）
    fail_on_failed: bool,
    consumed_calls: Arc<AtomicUsize>,
    failed_calls: Arc<AtomicUsize>,
    /// `on_failed` 收到的 RetryDecision（`None` = 未被回调）
    sees_decision: Arc<std::sync::Mutex<Option<RetryDecision>>>,
    /// `on_consumed` 时刻队列里 in_progress 的事件数（应为 1 = 尚未 ack）
    in_progress_at_consumed: Arc<AtomicUsize>,
    registry: Arc<Registry>,
    consumer_name: String,
}

#[async_trait]
impl Producer for ProbeProducer {
    fn name(&self) -> &str {
        "probe_producer"
    }
    fn topic(&self) -> EventTopic {
        EventTopic::TaskStatusChanged
    }
    async fn on_consumed(&self, _ctx: &RequestContext, _event: &serde_json::Value) -> Result<()> {
        self.consumed_calls.fetch_add(1, Ordering::SeqCst);
        // 不变量 1：业务回调**先于** queue.ack → 此刻事件仍在 in_progress
        let in_progress = self
            .registry
            .queue_stats(&self.consumer_name)
            .map(|s| s.in_progress_count)
            .unwrap_or(0);
        self.in_progress_at_consumed
            .store(in_progress, Ordering::SeqCst);
        Ok(())
    }
    async fn on_failed(
        &self,
        _ctx: &RequestContext,
        _event: &serde_json::Value,
        _err: &str,
        decision: RetryDecision,
        _attempt: u32,
    ) -> Result<()> {
        self.failed_calls.fetch_add(1, Ordering::SeqCst);
        *self.sees_decision.lock().expect("probe decision poisoned") = Some(decision);
        if self.fail_on_failed {
            return Err(err!(
                Internal,
                "probe producer on_failed intentional failure"
            ));
        }
        Ok(())
    }
    async fn start(&self, _sink: EventSink) -> Result<()> {
        Ok(())
    }
}

fn new_test_registry() -> Arc<Registry> {
    Arc::new(Registry::new())
}

/// 等一个原子计数达到期望值（避免测试靠固定 sleep 撞运气）
async fn wait_for(counter: &AtomicUsize, expected: usize, what: &str) {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    while counter.load(Ordering::SeqCst) < expected {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timeout waiting for {what}"
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}

/// 直接调私有收尾出口，构造 meta / 封套（不经 worker，断言确定）
fn probe_meta() -> AopEventMeta {
    AopEventMeta {
        event_id: "probe-1".to_string(),
        event_kind: EventTopic::TaskStatusChanged.as_str().to_string(),
        order_key: "agent-1".to_string(),
        priority: 0,
        created_at: 0,
        attempt: 1,
        context_carrier: None,
    }
}

fn probe_event_json() -> serde_json::Value {
    serde_json::json!({
        "id": "probe-1",
        "event_id": "probe-1",
        "kind": EventTopic::TaskStatusChanged.as_str(),
        "order_key": "agent-1",
    })
}

fn probe_producer(registry: &Arc<Registry>, fail_on_failed: bool) -> Arc<ProbeProducer> {
    Arc::new(ProbeProducer {
        fail_on_failed,
        consumed_calls: Arc::new(AtomicUsize::new(0)),
        failed_calls: Arc::new(AtomicUsize::new(0)),
        sees_decision: Arc::new(std::sync::Mutex::new(None)),
        in_progress_at_consumed: Arc::new(AtomicUsize::new(0)),
        registry: Arc::clone(registry),
        consumer_name: "probe_consumer".to_string(),
    })
}

/// 优雅退出：shutdown_all 后 worker 退出循环，不再消费新事件；producer.stop 被调用
#[tokio::test]
async fn shutdown_all_stops_workers_and_producers() {
    // publish 链路会构造 RequestContext::new_system，依赖全局 Storage
    crate::pkg::storage::test_support::init_for_test().await;
    let registry = new_test_registry();
    let consumed = Arc::new(AtomicUsize::new(0));
    registry
        .register_consumer(Arc::new(CountingConsumer {
            consumed: consumed.clone(),
        }))
        .unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    registry
        .register_producer(Arc::new(StoppableProducer {
            started: started.clone(),
            stopped: stopped.clone(),
        }))
        .unwrap();
    assert!(
        !started.load(Ordering::SeqCst),
        "start() 应在 start_all 时才被调用"
    );

    registry.start_all().await.unwrap();

    // 第一条事件被正常消费
    registry
        .publish(
            &RequestContext::new_system(),
            ShutdownTestEvent {
                id: "evt-1".to_string(),
            },
        )
        .await;
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    while consumed.load(Ordering::SeqCst) == 0 {
        assert!(tokio::time::Instant::now() < deadline, "event not consumed");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    assert!(
        started.load(Ordering::SeqCst),
        "producer.start(sink) not called"
    );

    registry.shutdown_all().await.unwrap();
    assert!(registry.is_shutting_down());
    assert!(stopped.load(Ordering::SeqCst), "producer.stop() not called");

    // 等 worker 退出（空队列休眠 20ms，给足 1s）
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // 停机后新事件留在队列中，不再被消费
    registry
        .publish(
            &RequestContext::new_system(),
            ShutdownTestEvent {
                id: "evt-2".to_string(),
            },
        )
        .await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    assert_eq!(
        consumed.load(Ordering::SeqCst),
        1,
        "worker should have exited"
    );
    assert_eq!(registry.queue_len("shutdown_test_consumer"), 1);
}

/// §6.7：同一 topic 被两个生产者声明 → 第二个注册必须 Err（不是静默覆盖）
#[tokio::test]
async fn register_producer_rejects_duplicate_topic() {
    let registry = new_test_registry();

    registry
        .register_producer(probe_producer(&registry, false))
        .unwrap();

    // 第二个生产者声明同一 topic（不同 name）→ 必须被拒
    let err = registry
        .register_producer(probe_producer(&registry, false))
        .expect_err("duplicate topic must be rejected");
    assert!(
        err.to_string().contains("1 topic : 1 producer"),
        "unexpected error: {err}"
    );
    assert_eq!(registry.producer_count(), 1);
}

/// §6.1-1：Sync 消费者声明 `ordered` = 谎言（无队列无门闩）→ 注册期硬拒
#[tokio::test]
async fn register_consumer_rejects_sync_ordered_subscription() {
    struct SyncOrderedConsumer;

    #[async_trait]
    impl Consumer for SyncOrderedConsumer {
        fn name(&self) -> &str {
            "sync_ordered"
        }
        fn subscriptions(&self) -> Vec<Subscription> {
            vec![Subscription::new(EventTopic::TaskStatusChanged).ordered()]
        }
        async fn on_event(&self, _ctx: RequestContext, _event: serde_json::Value) -> Result<()> {
            Ok(())
        }
    }

    let registry = new_test_registry();
    let err = registry
        .register_consumer(Arc::new(SyncOrderedConsumer))
        .expect_err("Sync consumer declaring ordered must be rejected");
    assert!(
        err.to_string().contains("must not declare ordered"),
        "unexpected error: {err}"
    );
}

/// §6.2：声明了 `notify_producer` 却没人注册生产者 → `start_all` 必须启动失败
///
/// 否则消费完成后无处回调，业务收尾（状态翻转 / 游标推进）静默丢失。
#[tokio::test]
async fn start_all_errs_when_notify_producer_has_no_producer() {
    crate::pkg::storage::test_support::init_for_test().await;

    let registry = new_test_registry();
    registry
        .register_consumer(Arc::new(ProbeConsumer {
            fail: false,
            decision: RetryDecision::Retry,
        }))
        .unwrap();

    let err = registry
        .start_all()
        .await
        .expect_err("start_all must fail without the producer");
    assert!(
        err.to_string().contains("notify_producer"),
        "unexpected error: {err}"
    );

    // 校验先于置位 started → 补上生产者后仍能正常启动（不会因 started 已置位而静默跳过）
    registry
        .register_producer(probe_producer(&registry, false))
        .unwrap();
    registry.start_all().await.unwrap();
    registry.shutdown_all().await.unwrap();
}

/// 成功路径：`on_consumed` 被调用，且**先于** `queue.ack`（回调时事件仍在 in_progress）
#[tokio::test]
async fn on_consumed_runs_before_queue_ack() {
    crate::pkg::storage::test_support::init_for_test().await;

    let registry = new_test_registry();
    registry
        .register_consumer(Arc::new(ProbeConsumer {
            fail: false,
            decision: RetryDecision::Retry,
        }))
        .unwrap();
    let producer = probe_producer(&registry, false);
    registry.register_producer(producer.clone()).unwrap();
    registry.start_all().await.unwrap();

    registry
        .publish(
            &RequestContext::new_system(),
            ProbeEvent {
                id: "evt-ok".to_string(),
            },
        )
        .await;

    wait_for(&producer.consumed_calls, 1, "on_consumed").await;
    assert_eq!(producer.failed_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        producer.in_progress_at_consumed.load(Ordering::SeqCst),
        1,
        "业务回调必须在 queue.ack 之前跑（此刻事件仍在 in_progress）"
    );

    // 结论是 Ack → 事件最终从队列移除
    wait_for_no_queue(&registry, "probe_consumer").await;

    registry.shutdown_all().await.unwrap();
}

/// 失败 + 消费者判 `Retry` → `Nack`（事件留在队列等待重投）
#[tokio::test]
async fn retry_decision_returns_nack() {
    // `finish_consumption` 会用 `carried_ctx` 还原同源 ctx（封套无 carrier 时回退
    // `RequestContext::new_system()`）→ 依赖全局 Storage
    crate::pkg::storage::test_support::init_for_test().await;

    let registry = new_test_registry();
    let consumer: Arc<dyn Consumer> = Arc::new(ProbeConsumer {
        fail: true,
        decision: RetryDecision::Retry,
    });
    let producer = probe_producer(&registry, false);
    registry.register_producer(producer.clone()).unwrap();

    let outcome = finish_consumption(
        &registry,
        &consumer,
        &probe_meta(),
        &probe_event_json(),
        Err("boom".to_string()),
    )
    .await;

    assert_eq!(outcome, DeliveryOutcome::Nack);
    assert_eq!(producer.failed_calls.load(Ordering::SeqCst), 1);
    assert_eq!(producer.consumed_calls.load(Ordering::SeqCst), 0);
    // 生产者是**收到**结论的一方，不是给出结论的一方
    assert_eq!(
        *producer.sees_decision.lock().unwrap(),
        Some(RetryDecision::Retry)
    );
}

/// 失败 + `Discard` → `Ack`（走 queue.ack 路径移除事件），且**仍回调** `on_consumed`
///
/// 后者是必须的：否则游标型生产者跨不过这条坏事件，下轮会重新拉到同一封 → 死循环。
#[tokio::test]
async fn discard_decision_returns_ack_and_still_calls_on_consumed() {
    crate::pkg::storage::test_support::init_for_test().await;

    let registry = new_test_registry();
    let consumer: Arc<dyn Consumer> = Arc::new(ProbeConsumer {
        fail: true,
        decision: RetryDecision::Discard,
    });
    let producer = probe_producer(&registry, false);
    registry.register_producer(producer.clone()).unwrap();

    let outcome = finish_consumption(
        &registry,
        &consumer,
        &probe_meta(),
        &probe_event_json(),
        Err("permanent".to_string()),
    )
    .await;

    assert_eq!(outcome, DeliveryOutcome::Ack);
    assert_eq!(producer.failed_calls.load(Ordering::SeqCst), 1);
    assert_eq!(producer.consumed_calls.load(Ordering::SeqCst), 1);
}

/// 生产者的收尾**写失败**不改变投递结论 —— 那是消费者的判定，不是生产者的
///
/// 语义变更（2026-09-17）：判定权下沉前，`Producer::on_failed` 返回 `Err` 会被
/// 当成「无法决策」→ 退化为 `Retry`（安全方向）。现在结论由 `Consumer::decide_retry`
/// 给出，生产者写失败只记日志，数据层面的一致性问题交给启动恢复兜底。
#[tokio::test]
async fn producer_settlement_error_does_not_change_decision() {
    crate::pkg::storage::test_support::init_for_test().await;

    let registry = new_test_registry();
    let consumer: Arc<dyn Consumer> = Arc::new(ProbeConsumer {
        fail: true,
        decision: RetryDecision::Discard,
    });
    // 收尾回调自身报错
    let producer = probe_producer(&registry, true);
    registry.register_producer(producer.clone()).unwrap();

    let outcome = finish_consumption(
        &registry,
        &consumer,
        &probe_meta(),
        &probe_event_json(),
        Err("boom".to_string()),
    )
    .await;

    assert_eq!(
        outcome,
        DeliveryOutcome::Ack,
        "消费者判 Discard → 结论就是 Ack，不因生产者写失败而退回 Retry"
    );
    // 但仍然照常回调 on_consumed（游标型生产者靠它跨过坏条目）
    assert_eq!(producer.consumed_calls.load(Ordering::SeqCst), 1);
}

/// 消费者的订阅**没声明** `notify_producer` → 不回调（①类纯通知保留零回调语义）
#[tokio::test]
async fn callback_skipped_without_notify_producer_declaration() {
    let registry = new_test_registry();
    let producer = probe_producer(&registry, false);
    registry.register_producer(producer.clone()).unwrap();

    // CountingConsumer 订阅 AgentStateChanged 且未声明 notify_producer
    let consumer: Arc<dyn Consumer> = Arc::new(CountingConsumer {
        consumed: Arc::new(AtomicUsize::new(0)),
    });
    let mut meta = probe_meta();
    meta.event_kind = EventTopic::AgentStateChanged.as_str().to_string();

    let outcome =
        finish_consumption(&registry, &consumer, &meta, &probe_event_json(), Ok(())).await;

    assert_eq!(outcome, DeliveryOutcome::Ack);
    assert_eq!(producer.consumed_calls.load(Ordering::SeqCst), 0);
}

/// 反查落空（无生产者的 topic）= ①类纯通知 → 跳过回调，但**消费者的终局判定照常生效**
///
/// ⚠️ 这是 2026-09-17 判定权下沉的关键护栏：过去「没有生产者」就等于「`Err` 无限
/// 重投」，逼得每个会上报 Err 的 topic 都得配一个只为兜次数的空生产者。现在判定在
/// 消费者侧，无生产者的 topic 也能在达到上限时正常放弃。
#[tokio::test]
async fn decision_applies_even_when_topic_has_no_producer() {
    let registry = new_test_registry();
    let consumer: Arc<dyn Consumer> = Arc::new(ProbeConsumer {
        fail: true,
        decision: RetryDecision::Discard,
    });

    let outcome = finish_consumption(
        &registry,
        &consumer,
        &probe_meta(),
        &probe_event_json(),
        Err("boom".to_string()),
    )
    .await;

    assert_eq!(
        outcome,
        DeliveryOutcome::Ack,
        "无生产者也必须按消费者的判定 ack，否则会无限重投"
    );
}

/// `EventSink` 只能发自己 topic 的事件（「1 producer : 1 topic」的执行点）
#[tokio::test]
async fn event_sink_rejects_foreign_topic() {
    crate::pkg::storage::test_support::init_for_test().await;

    let registry = new_test_registry();
    let sink = EventSink::new(Arc::clone(&registry), EventTopic::TaskStatusChanged);
    let ctx = RequestContext::new_system();

    // 自己的 topic → 放行
    sink.emit(
        &ctx,
        ProbeEvent {
            id: "ok".to_string(),
        },
    )
    .await
    .unwrap();

    // 别人的 topic → Err
    let err = sink
        .emit(
            &ctx,
            ShutdownTestEvent {
                id: "foreign".to_string(),
            },
        )
        .await
        .expect_err("foreign topic must be rejected");
    assert!(
        err.to_string().contains("cannot emit"),
        "unexpected error: {err}"
    );
}

/// `ProducerLoop` 的可中断休眠：`stop()` 后 sleep 必须在下一片返回 false
#[tokio::test]
async fn producer_loop_sleep_is_interruptible() {
    let loop_ctl = Arc::new(ProducerLoop::new());
    let stopper = Arc::clone(&loop_ctl);

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        stopper.stop();
    });

    let start = tokio::time::Instant::now();
    let still_running = loop_ctl.sleep(tokio::time::Duration::from_secs(30)).await;

    assert!(
        !still_running,
        "stop() 后 sleep 必须返回 false 让 loop 退出"
    );
    assert!(
        start.elapsed() < tokio::time::Duration::from_secs(2),
        "必须按片中断，而不是睡满 30s"
    );
}

/// 等消费者队列排空（Ack 之后事件应从 events 里移除）
async fn wait_for_no_queue(registry: &Arc<Registry>, consumer_name: &str) {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    while registry.queue_len(consumer_name) != 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timeout waiting for {consumer_name} queue to drain"
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}
