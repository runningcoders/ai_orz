use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use crate::pkg::logging::LogFields;
use crate::pkg::request_context::{AOP_CONTEXT_CARRIER_KEY, ContextCarrier, RequestContext};
use common::error::{Result, err};

use super::metrics_hook::{AopEventMeta, AopMetricsHook};
use super::{ConsumeMode, Consumer, Event, EventSink, Producer, RetryDecision};
use crate::pkg::aop::queue::{EventQueue, InMemoryEventQueue};
use common::enums::EventTopic;
use tracing::Level;

pub struct Registry {
    consumers: RwLock<HashMap<EventTopic, Vec<Arc<dyn Consumer>>>>,
    /// topic → 生产者索引：按 `Event::kind()` 反查归属的**唯一**依据
    ///
    /// 不变量 **1 topic : 1 producer**（`register_producer` 硬校验）—— 这正是
    /// 「按 kind 反查归属」得以成立的前提，也是 27 处 `publish` 调用点零改动的原因。
    producers_by_topic: RwLock<HashMap<EventTopic, Arc<dyn Producer>>>,
    queues: RwLock<HashMap<String, Arc<dyn EventQueue>>>,
    started: RwLock<bool>,
    /// 停机标志：异步 worker 每轮检查，true 时退出循环（优雅退出）
    shutting_down: AtomicBool,
    /// 指标采集 Hook（业务层注入，None 时零开销）
    metrics_hook: RwLock<Option<Arc<dyn AopMetricsHook>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            consumers: RwLock::new(HashMap::new()),
            producers_by_topic: RwLock::new(HashMap::new()),
            queues: RwLock::new(HashMap::new()),
            started: RwLock::new(false),
            shutting_down: AtomicBool::new(false),
            metrics_hook: RwLock::new(None),
        }
    }

    /// 注入指标采集 Hook（业务层在启动时调用）
    pub fn set_metrics_hook(&self, hook: Arc<dyn AopMetricsHook>) {
        let mut guard = self.metrics_hook.write().unwrap();
        *guard = Some(hook);
    }

    /// 读取 hook（内部辅助方法，None 时返回 None）
    fn metrics_hook(&self) -> Option<Arc<dyn AopMetricsHook>> {
        self.metrics_hook.read().ok()?.clone()
    }

    pub fn register_consumer(&self, consumer: Arc<dyn Consumer>) -> Result<()> {
        let name = consumer.name().to_string();
        let mode = consumer.consume_mode();
        let subscriptions = consumer.subscriptions();

        // 注册期硬校验：Sync 消费者内联执行、无队列无门闩，声明 ordered 是谎言。
        // 「声明了却静默不生效」比「没有这个保证」更危险（会让人据此写出依赖串行的代码），
        // 所以宁可注册失败。
        if mode == ConsumeMode::Sync && subscriptions.iter().any(|s| s.ordered) {
            return Err(err!(
                InvalidRequest,
                "sync consumer {} must not declare ordered subscription (no queue, no gate)",
                name
            ));
        }

        if mode == ConsumeMode::Async {
            let queue: Arc<dyn EventQueue> = Arc::new(InMemoryEventQueue::new());
            self.queues
                .write()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?
                .insert(name.clone(), queue);
        }

        let mut consumers = self
            .consumers
            .write()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        for subscription in subscriptions {
            consumers
                .entry(subscription.kind)
                .or_insert_with(Vec::new)
                .push(consumer.clone());
        }

        Ok(())
    }

    /// 注册生产者（装配期主动调用，**纯 push 的同步函数**）
    ///
    /// 随 `Producer::register` 的删除，它不再把 `Arc<Registry>` 反向注入生产者
    /// （发布句柄改由 `start_all` 注入的 [`EventSink`] 提供）→ 退化为一次 map 写入，
    /// 所以装配期（如 `dal::init()`）可以直接同步调用。
    ///
    /// ⚠️ 同一 `topic` 已被别的生产者占用 → **返回 Err**。若静默覆盖，另一个生产者
    /// 的事件回调会落到**错误的生产者身上**（例如 cron 的 `mark_trigger_executed`
    /// 被无关生产者接管 → 触发器永不 mark → 反复重跑）。注意 topic 枚举化防不住这条：
    /// 枚举保证「值合法」，不保证「只用一次」。
    pub fn register_producer(&self, producer: Arc<dyn Producer>) -> Result<()> {
        let topic = producer.topic();
        let name = producer.name().to_string();

        let mut producers = self
            .producers_by_topic
            .write()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        if let Some(existing) = producers.get(&topic) {
            return Err(err!(
                Conflict,
                "topic {} is already owned by producer {} — cannot register {}; 1 topic : 1 producer",
                topic,
                existing.name(),
                name
            ));
        }

        producers.insert(topic, producer);
        Ok(())
    }

    /// 按 topic 反查生产者（落空 = ①类纯通知，跳过生产者回调）
    fn producer_for(&self, kind: EventTopic) -> Option<Arc<dyn Producer>> {
        self.producers_by_topic.read().ok()?.get(&kind).cloned()
    }

    /// 快照全部生产者（注册期顺序不确定 → 按 name 排序，保证启动日志稳定）
    fn producers_snapshot(&self) -> Result<Vec<Arc<dyn Producer>>> {
        let producers = self
            .producers_by_topic
            .read()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        let mut result: Vec<Arc<dyn Producer>> = producers.values().cloned().collect();
        result.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(result)
    }

    pub async fn publish<E: Event>(&self, ctx: &RequestContext, event: E) {
        let kind = event.kind();

        let interested = {
            let consumers = match self.consumers.read() {
                Ok(c) => c,
                Err(e) => {
                    sys_error!("registry read error: {}", e);
                    return;
                }
            };

            consumers.get(&kind).cloned()
        };

        let Some(interested) = interested else {
            return;
        };

        // 在序列化前提取元字段
        let event_id = event.id().to_string();
        let event_kind = event.kind().as_str().to_string();
        let order_key = event.order_key().to_string();
        let priority = event.priority();
        let created_at = event.created_at();

        let mut event_json = match serde_json::to_value(event) {
            Ok(v) => v,
            Err(e) => {
                sys_error!("event serialize error: {}", e);
                return;
            }
        };

        // 统一注入元字段到 JSON 顶层，确保队列和监控能读取到一致的元数据
        if let Some(obj) = event_json.as_object_mut() {
            obj.entry("event_id")
                .or_insert(serde_json::Value::String(event_id));
            obj.entry("kind")
                .or_insert(serde_json::Value::String(event_kind));
            obj.entry("order_key")
                .or_insert(serde_json::Value::String(order_key));
            obj.entry("priority").or_insert(serde_json::json!(priority));
            obj.entry("created_at")
                .or_insert(serde_json::json!(created_at));
        }

        // 注入可传输子 context（context propagation）：抽取生产者 ctx 的可序列化
        // 标识字段随事件流转，消费侧据此重建出同源 ctx（见 carried_ctx），
        // 从而把整条链路（log_id 等）串联起来。
        if let Some(obj) = event_json.as_object_mut() {
            obj.insert(
                AOP_CONTEXT_CARRIER_KEY.to_string(),
                serde_json::to_value(ctx.to_carrier()).unwrap_or(serde_json::Value::Null),
            );
        }

        for consumer in interested {
            if !consumer.should_consume(&event_json).await {
                continue;
            }

            // 埋点：on_publish
            let meta = AopEventMeta::from_json(&event_json);
            let is_async = matches!(consumer.consume_mode(), ConsumeMode::Async);
            if let Some(hook) = self.metrics_hook() {
                hook.on_publish(consumer.name(), &meta, is_async);
            }

            match consumer.consume_mode() {
                ConsumeMode::Sync => {
                    // 同步：直接调用 on_event
                    let start = std::time::Instant::now();
                    if let Some(hook) = self.metrics_hook() {
                        hook.on_consume_start(consumer.name(), &meta);
                    }
                    let ctx = Self::carried_ctx(&event_json);
                    // 进入链路 span：将还原 ctx 的 log_id 等字段挂到当前上下文，
                    // 使消费者内部的所有日志（如 agent loop started/finished）自动携带。
                    let span = ctx.create_log_span(consumer.name(), Level::INFO);
                    let _span_guard = span.enter();
                    let outcome = match consumer.on_event(ctx, event_json.clone()).await {
                        Ok(()) => {
                            let duration_ms = start.elapsed().as_millis() as u64;
                            if let Some(hook) = self.metrics_hook() {
                                hook.on_consume_success(consumer.name(), &meta, duration_ms);
                            }
                            Ok(())
                        }
                        Err(e) => {
                            let duration_ms = start.elapsed().as_millis() as u64;
                            let err_str = format!("{:?}", e);
                            sys_error!("consumer {} sync error: {}", consumer.name(), e);
                            if let Some(hook) = self.metrics_hook() {
                                hook.on_consume_failure(
                                    consumer.name(),
                                    &meta,
                                    duration_ms,
                                    &err_str,
                                );
                            }
                            Err(err_str)
                        }
                    };
                    // Sync 内联执行、无队列 → 投递结论没有重投驱动者（`Retry` 在此无意义），
                    // 但仍经同一收尾出口：生产者回调与结论判定只有一个地方。
                    let _ = finish_consumption(self, &consumer, &meta, &event_json, outcome).await;
                }
                ConsumeMode::Async => {
                    let queue = {
                        let queues = match self.queues.read() {
                            Ok(q) => q,
                            Err(e) => {
                                sys_error!("registry read error: {}", e);
                                continue;
                            }
                        };
                        queues.get(consumer.name()).cloned()
                    };

                    if let Some(queue) = queue {
                        let ctx = RequestContext::new_system();
                        if let Err(e) = queue.enqueue(ctx, event_json.clone()).await {
                            sys_error!("consumer {} enqueue error: {}", consumer.name(), e);
                        }
                    }
                }
            }
        }
    }

    /// 从事件 JSON 还原消费侧 ctx。
    ///
    /// 提取顶层 `context_carrier` 并重建成与主 context 同源的 [`RequestContext`]
    /// （保留 log_id 等链路标识）；缺失或解析失败时回退为 system ctx。
    fn carried_ctx(event_json: &serde_json::Value) -> RequestContext {
        match ContextCarrier::from_json(event_json) {
            Some(c) => c.into_context(),
            None => {
                // 事件缺失/损坏 context_carrier：无法还原生产者链路标识，
                // 回退 new_system() 会重新生成 log_id（丢上下文）。显式告警，避免静默换号。
                sys_warn!(
                    "AOP event 缺少有效的 context_carrier，回退 new_system()（log_id 将重新生成，链路不可串联）"
                );
                RequestContext::new_system()
            }
        }
    }

    pub async fn dequeue_for(&self, consumer_name: &str) -> Result<Option<serde_json::Value>> {
        let queue = {
            let queues = self
                .queues
                .read()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

            queues
                .get(consumer_name)
                .ok_or_else(|| err!(NotFound, "consumer queue not found: {}", consumer_name))?
                .clone()
        };

        let ctx = RequestContext::new_system();
        let value = queue.dequeue_next(ctx).await?;
        Ok(value)
    }

    pub async fn ack(&self, consumer_name: &str, event_id: &str) -> Result<()> {
        let queue = {
            let queues = self
                .queues
                .read()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

            queues
                .get(consumer_name)
                .ok_or_else(|| err!(NotFound, "consumer queue not found: {}", consumer_name))?
                .clone()
        };

        let ctx = RequestContext::new_system();
        queue.ack(ctx, event_id).await
    }

    pub async fn nack(&self, consumer_name: &str, event_id: &str) -> Result<()> {
        let queue = {
            let queues = self
                .queues
                .read()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

            queues
                .get(consumer_name)
                .ok_or_else(|| err!(NotFound, "consumer queue not found: {}", consumer_name))?
                .clone()
        };

        let ctx = RequestContext::new_system();
        queue.nack(ctx, event_id).await
    }

    pub async fn start_all(self: &Arc<Self>) -> Result<()> {
        // ⚠️ 校验必须**先于**置位 `started`：本函数失败后若有人重试，`started` 已置位
        // 会直接 `return Ok(())` —— 变成「静默不启动」，比启动失败更难查。
        self.ensure_producers_for_notifying_consumers()?;

        // 原子地检查并标记 started，立即释放写锁，避免在持有锁的情况下
        // 跨 await 点（producer.start(sink).await 等）导致死锁。
        // 语义：start_all 只能成功执行一次；后续调用直接返回。
        // 失败时不回退标记——与原逻辑一致（已 spawn 的 worker 无法回收）。
        {
            let mut started = self
                .started
                .write()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

            if *started {
                return Ok(());
            }
            *started = true;
        }

        let async_consumers: Vec<Arc<dyn Consumer>> = {
            let consumers = self
                .consumers
                .read()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

            let mut seen = std::collections::HashSet::new();
            let mut result = Vec::new();

            for consumer_list in consumers.values() {
                for consumer in consumer_list {
                    if consumer.consume_mode() == ConsumeMode::Async
                        && seen.insert(consumer.name().to_string())
                    {
                        result.push(consumer.clone());
                    }
                }
            }

            result
        };

        for consumer in async_consumers {
            let name = consumer.name().to_string();
            let concurrency = consumer.concurrency();
            let empty_sleep = consumer.empty_queue_sleep_ms();
            let error_sleep = consumer.error_retry_sleep_ms();

            let has_queue = {
                let queues = self
                    .queues
                    .read()
                    .map_err(|e| err!(Internal, "registry lock error: {}", e))?;
                queues.contains_key(&name)
            };

            if !has_queue {
                sys_warn!("consumer {} has no queue, skip starting workers", name);
                continue;
            }

            for worker_id in 0..concurrency {
                let consumer = consumer.clone();
                // `self: &Arc<Self>` → 直接 clone 即可：self_ref 那套「从 &self 造出
                // Arc<Self>」的绕行已随 register / 轮询段的删除一并消失。
                let registry_arc = Arc::clone(self);

                let consumer_name = name.clone();
                tokio::spawn(async move {
                    sys_info!("[{}] worker {} started", consumer_name, worker_id);
                    loop {
                        // 优雅退出：停机标志置位后，当前事件处理完即退出
                        if registry_arc.is_shutting_down() {
                            sys_info!("[{}] worker {} shutting down", consumer_name, worker_id);
                            break;
                        }
                        match registry_arc.dequeue_for(&consumer_name).await {
                            Ok(Some(event_json)) => {
                                let event_id = event_json
                                    .get("event_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();

                                // 提取元信息 + 记录开始时间
                                let meta = AopEventMeta::from_json(&event_json);
                                let start = std::time::Instant::now();

                                // 埋点：on_consume_start
                                if let Some(hook) = registry_arc.metrics_hook() {
                                    hook.on_consume_start(&consumer_name, &meta);
                                }

                                let ctx = Self::carried_ctx(&event_json);
                                // 进入链路 span：将还原 ctx 的 log_id 等字段挂到当前上下文，
                                // 使消费者内部的所有日志自动携带（与 HTTP 层 log_info! 同机制）。
                                let span = ctx.create_log_span(&consumer_name, Level::INFO);
                                let _span_guard = span.enter();
                                // 事件 JSON 在收尾（生产者回调）时还要用 → 传副本给消费者，
                                // 原件留给 finish_consumption（它要拿 event 喂 on_consumed）
                                let outcome = match consumer.on_event(ctx, event_json.clone()).await
                                {
                                    Ok(()) => {
                                        let duration_ms = start.elapsed().as_millis() as u64;
                                        // 埋点：on_consume_success
                                        if let Some(hook) = registry_arc.metrics_hook() {
                                            hook.on_consume_success(
                                                &consumer_name,
                                                &meta,
                                                duration_ms,
                                            );
                                        }
                                        Ok(())
                                    }
                                    Err(e) => {
                                        let duration_ms = start.elapsed().as_millis() as u64;
                                        let err_str = format!("{:?}", e);
                                        sys_error!(
                                            "[{}] on_event error for {}: {}",
                                            consumer_name,
                                            event_id,
                                            e
                                        );
                                        // 埋点：on_consume_failure
                                        if let Some(hook) = registry_arc.metrics_hook() {
                                            hook.on_consume_failure(
                                                &consumer_name,
                                                &meta,
                                                duration_ms,
                                                &err_str,
                                            );
                                        }
                                        Err(err_str)
                                    }
                                };

                                // 投递结论由 finish_consumption 单一判定：它同时触发生产者的
                                // on_consumed / on_failed，并把 RetryDecision 折算成 ack / nack。
                                // 业务收尾（如 messages 状态翻转）已在回调里完成 → 这里只落队列。
                                match finish_consumption(
                                    &registry_arc,
                                    &consumer,
                                    &meta,
                                    &event_json,
                                    outcome,
                                )
                                .await
                                {
                                    DeliveryOutcome::Ack => {
                                        // 必须调用 queue.ack 从内存队列移除事件
                                        // 否则事件永远停留在 in_progress + events，
                                        // 导致同 order_key 后续消息卡死（has_active_message 永远 true）
                                        if let Err(e) =
                                            registry_arc.ack(&consumer_name, &event_id).await
                                        {
                                            sys_error!(
                                                "[{}] queue.ack error for {}: {}",
                                                consumer_name,
                                                event_id,
                                                e
                                            );
                                        }
                                    }
                                    DeliveryOutcome::Nack => {
                                        // 必须调用 queue.nack 让事件重新入队等待重试
                                        // 否则失败事件永远停留在 in_progress，无法重试
                                        if let Err(e) =
                                            registry_arc.nack(&consumer_name, &event_id).await
                                        {
                                            sys_error!(
                                                "[{}] queue.nack error for {}: {}",
                                                consumer_name,
                                                event_id,
                                                e
                                            );
                                        }
                                        // 退避：on_event 失败后添加 sleep，避免紧密自旋
                                        // 之前 error_sleep 只用于 dequeue_for 失败，不用于 on_event 失败
                                        // 导致 Agent busy 时 nack 后立即重新入队被取出，形成 CPU 紧密自旋
                                        tokio::time::sleep(tokio::time::Duration::from_millis(
                                            error_sleep,
                                        ))
                                        .await;
                                    }
                                }
                            }
                            Ok(None) => {
                                tokio::time::sleep(tokio::time::Duration::from_millis(empty_sleep))
                                    .await;
                            }
                            Err(e) => {
                                sys_error!("[{}] dequeue error: {}", consumer_name, e);
                                tokio::time::sleep(tokio::time::Duration::from_millis(error_sleep))
                                    .await;
                            }
                        }
                    }
                });
            }

            sys_info!("[{}] started {} workers", name, concurrency);
        }

        // 逐个启动生产者：机制（自管 loop）在生产者，时机（何时 start / stop）由 AOP 统一管。
        // 框架不再代跑轮询 —— 原先那套「interval 切成 500ms 片段 + 每片自检停机标志」
        // 已收进 `ProducerLoop` 工具，由生产者用它实现自己的 loop。
        for producer in self.producers_snapshot()? {
            let name = producer.name().to_string();
            // 发布句柄预先绑定本生产者的 topic → 它发不出别人的事件
            let sink = EventSink::new(Arc::clone(self), producer.topic());

            if let Err(e) = producer.start(sink).await {
                sys_error!("[{}] start error: {}", name, e);
            }
            sys_info!("[{}] producer started (topic={})", name, producer.topic());
        }

        Ok(())
    }

    /// 启动期校验：声明了 `notify_producer` 的 topic **必须**有生产者
    ///
    /// 缺失即返回 Err（启动失败）—— 否则消费完成后无处回调，业务收尾（状态翻转 /
    /// 游标推进 / `mark_trigger_executed`）会**静默丢失**。
    /// 之所以放在这里而不是注册期：消费者与生产者的注册顺序不固定，注册期判不了。
    fn ensure_producers_for_notifying_consumers(&self) -> Result<()> {
        let consumers = self
            .consumers
            .read()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;
        let producers = self
            .producers_by_topic
            .read()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        let mut missing: Vec<String> = Vec::new();
        for (kind, interested) in consumers.iter() {
            let needs_callback = interested.iter().any(|c| {
                c.subscriptions()
                    .into_iter()
                    .any(|s| s.kind == *kind && s.notify_producer)
            });

            if needs_callback && !producers.contains_key(kind) {
                missing.push(kind.to_string());
            }
        }

        if missing.is_empty() {
            return Ok(());
        }

        missing.sort();
        Err(err!(
            ConfigMissing,
            "consumers declare notify_producer for {:?} but no producer is registered — 业务收尾将静默丢失",
            missing
        ))
    }

    pub fn consumer_count(&self) -> usize {
        self.consumers
            .read()
            .map(|c| c.values().map(|v| v.len()).sum())
            .unwrap_or(0)
    }

    /// 停机标志是否已置位（异步 worker 每轮检查；生产者用自己的 `ProducerLoop`）
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    /// 停机所有 worker 与 producer（优雅退出）
    ///
    /// 置位停机标志后，异步消费者 worker 在当前事件处理完毕后退出循环，
    /// 轮询 producer 最多在 500ms 内退出；随后逐个调用 `Producer::stop()`
    /// 停掉自管生命周期的 producer（如外部渠道 WS 监听）。
    pub async fn shutdown_all(&self) -> Result<()> {
        self.shutting_down.store(true, Ordering::SeqCst);

        // 这里就是「AOP 统一管理退出时机」：逐个 `stop()`，而 `stop()` 的契约是
        // 「置位并等 loop 真正退出」→ 返回后 loop 必已不再运行。
        for producer in self.producers_snapshot()? {
            if let Err(e) = producer.stop().await {
                sys_error!("[{}] producer stop error: {}", producer.name(), e);
            }
        }
        Ok(())
    }

    pub fn producer_count(&self) -> usize {
        self.producers_by_topic.read().map(|p| p.len()).unwrap_or(0)
    }

    /// 是否存在拥有该 topic 的生产者
    ///
    /// 观测/装配自检用（与 `producer_count` 同族）。它同时是「①类纯通知」的判据：
    /// 返回 `false` 的 topic 就是「没有归属、不需要业务回调」的纯通知。
    pub fn has_producer(&self, kind: EventTopic) -> bool {
        self.producers_by_topic
            .read()
            .map(|p| p.contains_key(&kind))
            .unwrap_or(false)
    }

    pub fn queue_len(&self, consumer_name: &str) -> usize {
        if let Ok(queues) = self.queues.read()
            && let Some(queue) = queues.get(consumer_name)
        {
            return queue.len();
        }
        0
    }

    /// 获取所有队列的聚合统计
    pub fn all_queue_stats(&self) -> Vec<(String, crate::pkg::aop::queue::QueueStats)> {
        let queues = match self.queues.read() {
            Ok(q) => q,
            Err(_) => return Vec::new(),
        };

        let mut result = Vec::new();
        for (name, queue) in queues.iter() {
            result.push((name.clone(), queue.stats()));
        }

        // 按队列名排序
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// 获取指定消费者的队列统计
    pub fn queue_stats(&self, consumer_name: &str) -> Option<crate::pkg::aop::queue::QueueStats> {
        let queues = self.queues.read().ok()?;
        let queue = queues.get(consumer_name)?;
        Some(queue.stats())
    }

    /// 查询指定消费者队列中的事件
    pub fn query_events(
        &self,
        consumer_name: &str,
        filter: crate::pkg::aop::queue::EventQueryFilter,
    ) -> Option<Vec<crate::pkg::aop::queue::EventSummary>> {
        let queues = self.queues.read().ok()?;
        let queue = queues.get(consumer_name)?;
        Some(queue.query_events(filter))
    }

    /// 获取指定消费者队列中的事件详情
    pub fn get_event(
        &self,
        consumer_name: &str,
        event_id: &str,
    ) -> Option<crate::pkg::aop::queue::EventDetail> {
        let queues = self.queues.read().ok()?;
        let queue = queues.get(consumer_name)?;
        queue.get_event(event_id)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// `on_event` 的投递结论（由 [`finish_consumption`] 判定，调用方负责落到队列上）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeliveryOutcome {
    /// 消费成功 → 队列 `ack`，事件移除
    Ack,
    /// 消费失败 → 队列 `nack`，按既有退避重投
    Nack,
}

/// 消费收尾 —— Sync 内联路径与 Async worker 路径的**唯一**收尾出口
///
/// 单一出口的意义：投递结论只在一个地方判定，避免「又长出两条路径」。它同时负责
/// 两件事：**触发生产者业务回调** + **给出投递结论**（调用方只管把结论落到队列上）。
///
/// 反查规则：按事件封套的 `kind` 解析 topic（未知 = 无生产者 = ①类纯通知 → 跳过回调），
/// 再要求**该消费者对本 kind 显式声明了 `notify_producer`** —— 两个条件都满足才回调。
///
/// 三条不变量（均已写进实现）：
/// 1. **业务回调先于 `queue.ack`**（与改造前 `consumer.ack` 先于 `registry.ack` 一致）
///    → 代价是回调必须幂等：回调成功、`queue.ack` 前崩溃 → 事件重投、回调再跑一次。
/// 2. `on_failed` 的触发时机 = **每次尝试失败**（不是终态通知）；
///    `Discard` 是生产者用它表达终态的方式。
/// 3. **回调自身失败不改变投递结论**（业务收尾是生产者的责任，不能靠「卡住队列」来重试）。
async fn finish_consumption(
    registry: &Registry,
    consumer: &Arc<dyn Consumer>,
    meta: &AopEventMeta,
    event_json: &serde_json::Value,
    outcome: std::result::Result<(), String>,
) -> DeliveryOutcome {
    // 反查归属：source 不再是硬编码字符串比较 —— 归属由 kind → producer 索引直接决定
    let producer = EventTopic::parse(&meta.event_kind)
        .filter(|kind| declares_notify_producer(consumer, *kind))
        .and_then(|kind| registry.producer_for(kind));

    let Some(producer) = producer else {
        return delivery_of(&outcome);
    };

    // 回调需要一份 ctx：worker 里 `on_event(ctx, ..)` 是按值 move，到此 ctx 已被移走
    // → 从封套还原同源 ctx（顺带修掉 P4：业务收尾不再跑在断链的 `new_system()` 上）
    let ctx = Registry::carried_ctx(event_json);

    match outcome {
        Ok(()) => {
            callback_consumed(&*producer, &ctx, event_json, consumer).await;
            DeliveryOutcome::Ack
        }
        Err(err) => {
            let decision = match producer
                .on_failed(&ctx, event_json, &err, meta.attempt)
                .await
            {
                Ok(decision) => decision,
                Err(e) => {
                    // 决策本身失败 → 安全方向：视为 Retry（宁可重投，绝不静默丢弃）
                    sys_error!(
                        "[{}] producer {} on_failed error, fallback to Retry: {}",
                        consumer.name(),
                        producer.name(),
                        e
                    );
                    RetryDecision::Retry
                }
            };

            match decision {
                RetryDecision::Retry => DeliveryOutcome::Nack,
                RetryDecision::Discard => {
                    // 本设计里**唯一不可逆**的动作：事件永久移除且无死信存储
                    // → 必须留下 error 日志与独立埋点（不得混入失败率，见 on_consume_discarded）
                    sys_error!(
                        "event DISCARDED by producer {}: consumer={} event_id={} attempt={} err={}",
                        producer.name(),
                        consumer.name(),
                        meta.event_id,
                        meta.attempt,
                        err
                    );
                    if let Some(hook) = registry.metrics_hook() {
                        hook.on_consume_discarded(consumer.name(), meta, &err);
                    }

                    // ⚠️ Discard 后**仍要**回调 on_consumed：否则游标型生产者
                    // （如 IMAP 游标）跨不过这条坏事件，下轮会重新拉到同一封，形成死循环。
                    callback_consumed(&*producer, &ctx, event_json, consumer).await;
                    DeliveryOutcome::Ack
                }
            }
        }
    }
}

/// 由 `on_event` 的 Result 直接得出投递结论（无生产者回调时的分支）
fn delivery_of(outcome: &std::result::Result<(), String>) -> DeliveryOutcome {
    match outcome {
        Ok(()) => DeliveryOutcome::Ack,
        Err(_) => DeliveryOutcome::Nack,
    }
}

/// 该消费者是否对某 topic 显式声明了 `notify_producer`
///
/// 归属靠 kind → producer 索引反查，**不需要**在消费者里再写一遍 `if kind != ..` 判断。
fn declares_notify_producer(consumer: &Arc<dyn Consumer>, kind: EventTopic) -> bool {
    consumer
        .subscriptions()
        .into_iter()
        .any(|s| s.kind == kind && s.notify_producer)
}

/// 触发生产者的业务收尾；失败只记日志，**不改变投递结论**
async fn callback_consumed(
    producer: &dyn Producer,
    ctx: &RequestContext,
    event_json: &serde_json::Value,
    consumer: &Arc<dyn Consumer>,
) {
    if let Err(e) = producer.on_consumed(ctx, event_json).await {
        sys_error!(
            "[{}] producer {} on_consumed error: {}",
            consumer.name(),
            producer.name(),
            e
        );
    }
}

#[cfg(test)]
mod tests {
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

    /// 声明了 `notify_producer` 的 Async 消费者；`fail` 控制 on_event 成败
    struct ProbeConsumer {
        fail: bool,
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
        async fn on_event(&self, _ctx: RequestContext, _event: serde_json::Value) -> Result<()> {
            if self.fail {
                return Err(err!(Internal, "probe consumer intentional failure"));
            }
            Ok(())
        }
    }

    /// 记录回调事实的生产者：既可验证 ack/重投/Discard，也可验证「回调先于 queue.ack」
    struct ProbeProducer {
        /// `on_failed` 的返回值
        decision: RetryDecision,
        /// `on_failed` 自身是否直接报错（验证「视为 Retry」）
        fail_on_failed: bool,
        consumed_calls: Arc<AtomicUsize>,
        failed_calls: Arc<AtomicUsize>,
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
        async fn on_consumed(
            &self,
            _ctx: &RequestContext,
            _event: &serde_json::Value,
        ) -> Result<()> {
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
            _attempt: u32,
        ) -> Result<RetryDecision> {
            self.failed_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_on_failed {
                return Err(err!(
                    Internal,
                    "probe producer on_failed intentional failure"
                ));
            }
            Ok(self.decision)
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

    fn probe_producer(
        registry: &Arc<Registry>,
        decision: RetryDecision,
        fail_on_failed: bool,
    ) -> Arc<ProbeProducer> {
        Arc::new(ProbeProducer {
            decision,
            fail_on_failed,
            consumed_calls: Arc::new(AtomicUsize::new(0)),
            failed_calls: Arc::new(AtomicUsize::new(0)),
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
            .register_producer(probe_producer(&registry, RetryDecision::Retry, false))
            .unwrap();

        // 第二个生产者声明同一 topic（不同 name）→ 必须被拒
        let err = registry
            .register_producer(probe_producer(&registry, RetryDecision::Retry, false))
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
            async fn on_event(
                &self,
                _ctx: RequestContext,
                _event: serde_json::Value,
            ) -> Result<()> {
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
            .register_consumer(Arc::new(ProbeConsumer { fail: false }))
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
            .register_producer(probe_producer(&registry, RetryDecision::Retry, false))
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
            .register_consumer(Arc::new(ProbeConsumer { fail: false }))
            .unwrap();
        let producer = probe_producer(&registry, RetryDecision::Retry, false);
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

    /// 失败 + `Retry` → `Nack`（事件留在队列等待重投）
    #[tokio::test]
    async fn retry_decision_returns_nack() {
        // `finish_consumption` 会用 `carried_ctx` 还原同源 ctx（封套无 carrier 时回退
        // `RequestContext::new_system()`）→ 依赖全局 Storage
        crate::pkg::storage::test_support::init_for_test().await;

        let registry = new_test_registry();
        let consumer: Arc<dyn Consumer> = Arc::new(ProbeConsumer { fail: true });
        let producer = probe_producer(&registry, RetryDecision::Retry, false);
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
    }

    /// 失败 + `Discard` → `Ack`（走 queue.ack 路径移除事件），且**仍回调** `on_consumed`
    ///
    /// 后者是必须的：否则游标型生产者跨不过这条坏事件，下轮会重新拉到同一封 → 死循环。
    #[tokio::test]
    async fn discard_decision_returns_ack_and_still_calls_on_consumed() {
        crate::pkg::storage::test_support::init_for_test().await;

        let registry = new_test_registry();
        let consumer: Arc<dyn Consumer> = Arc::new(ProbeConsumer { fail: true });
        let producer = probe_producer(&registry, RetryDecision::Discard, false);
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

    /// `on_failed` 自身报错 → 视为 `Retry`（安全方向：宁可重投，绝不静默丢弃）
    #[tokio::test]
    async fn on_failed_error_falls_back_to_retry() {
        crate::pkg::storage::test_support::init_for_test().await;

        let registry = new_test_registry();
        let consumer: Arc<dyn Consumer> = Arc::new(ProbeConsumer { fail: true });
        // 决策是 Discard，但 on_failed 会报错 → 必须退化为 Retry
        let producer = probe_producer(&registry, RetryDecision::Discard, true);
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
        assert_eq!(producer.consumed_calls.load(Ordering::SeqCst), 0);
    }

    /// 消费者的订阅**没声明** `notify_producer` → 不回调（①类纯通知保留零回调语义）
    #[tokio::test]
    async fn callback_skipped_without_notify_producer_declaration() {
        let registry = new_test_registry();
        let producer = probe_producer(&registry, RetryDecision::Retry, false);
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

    /// 反查落空（无生产者的 topic）= ①类纯通知 → 跳过回调，结论仍按 Result 给出
    #[tokio::test]
    async fn callback_skipped_when_topic_has_no_producer() {
        let registry = new_test_registry();
        let consumer: Arc<dyn Consumer> = Arc::new(ProbeConsumer { fail: true });

        let outcome = finish_consumption(
            &registry,
            &consumer,
            &probe_meta(),
            &probe_event_json(),
            Err("boom".to_string()),
        )
        .await;

        assert_eq!(outcome, DeliveryOutcome::Nack);
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
}
