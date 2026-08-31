use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, Weak};

use crate::pkg::logging::LogFields;
use crate::pkg::request_context::{AOP_CONTEXT_CARRIER_KEY, ContextCarrier, RequestContext};
use common::error::{Result, err};

use super::metrics_hook::{AopEventMeta, AopMetricsHook};
use super::{ConsumeMode, Consumer, Event, EventKind, Producer};
use crate::pkg::aop::queue::{EventQueue, InMemoryEventQueue};
use tracing::Level;

pub struct Registry {
    self_ref: RwLock<Option<Weak<Self>>>,
    consumers: RwLock<HashMap<EventKind, Vec<Arc<dyn Consumer>>>>,
    producers: RwLock<Vec<Arc<dyn Producer>>>,
    queues: RwLock<HashMap<String, Arc<dyn EventQueue>>>,
    started: RwLock<bool>,
    /// 停机标志：worker / producer 循环每轮检查，true 时退出循环（优雅退出）
    shutting_down: AtomicBool,
    /// 指标采集 Hook（业务层注入，None 时零开销）
    metrics_hook: RwLock<Option<Arc<dyn AopMetricsHook>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            self_ref: RwLock::new(None),
            consumers: RwLock::new(HashMap::new()),
            producers: RwLock::new(Vec::new()),
            queues: RwLock::new(HashMap::new()),
            started: RwLock::new(false),
            shutting_down: AtomicBool::new(false),
            metrics_hook: RwLock::new(None),
        }
    }

    pub fn set_self_ref(&self, arc: Arc<Self>) {
        let mut self_ref = self.self_ref.write().unwrap();
        *self_ref = Some(Arc::downgrade(&arc));
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

        if consumer.consume_mode() == ConsumeMode::Async {
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

        for kind in consumer.interested_events() {
            consumers
                .entry(kind)
                .or_insert_with(Vec::new)
                .push(consumer.clone());
        }

        Ok(())
    }

    pub async fn register_producer(&self, producer: Arc<dyn Producer>) -> Result<()> {
        let registry_arc = {
            let self_ref = self
                .self_ref
                .read()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?;
            self_ref
                .as_ref()
                .and_then(|w| w.upgrade())
                .ok_or_else(|| err!(Internal, "registry not initialized"))?
        };
        producer.register(registry_arc).await?;

        let mut producers = self
            .producers
            .write()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;
        producers.push(producer);

        Ok(())
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
        let event_kind = event.kind().0.to_string();
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
                    match consumer.on_event(ctx, event_json.clone()).await {
                        Ok(()) => {
                            let duration_ms = start.elapsed().as_millis() as u64;
                            if let Some(hook) = self.metrics_hook() {
                                hook.on_consume_success(consumer.name(), &meta, duration_ms);
                            }
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
                        }
                    }
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

    pub async fn start_all(&self) -> Result<()> {
        // 原子地检查并标记 started，立即释放写锁，避免在持有锁的情况下
        // 跨 await 点（producer.start().await 等）导致死锁。
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

                let registry_arc = {
                    let self_ref = self
                        .self_ref
                        .read()
                        .map_err(|e| err!(Internal, "registry lock error: {}", e))?;
                    self_ref
                        .as_ref()
                        .and_then(|w| w.upgrade())
                        .ok_or_else(|| err!(Internal, "registry self-ref not set"))?
                };

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
                                match consumer.on_event(ctx, event_json).await {
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
                                        if let Err(e) = consumer.ack(&event_id).await {
                                            sys_error!(
                                                "[{}] ack error for {}: {}",
                                                consumer_name,
                                                event_id,
                                                e
                                            );
                                        }
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
                                        if let Err(e) = consumer.nack(&event_id).await {
                                            sys_error!(
                                                "[{}] nack error for {}: {}",
                                                consumer_name,
                                                event_id,
                                                e
                                            );
                                        }
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

        let producers = {
            let producers = self
                .producers
                .read()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?;
            producers.clone()
        };

        for producer in producers {
            let name = producer.name().to_string();
            let interval = producer.poll_interval_secs();

            if interval > 0 {
                let producer = producer.clone();
                let registry_arc = {
                    let self_ref = self
                        .self_ref
                        .read()
                        .map_err(|e| err!(Internal, "registry lock error: {}", e))?;
                    self_ref
                        .as_ref()
                        .and_then(|w| w.upgrade())
                        .ok_or_else(|| err!(Internal, "registry self-ref not set"))?
                };
                tokio::spawn(async move {
                    sys_info!(
                        "[{}] polling producer started, interval: {}s",
                        name,
                        interval
                    );
                    loop {
                        if registry_arc.is_shutting_down() {
                            sys_info!("[{}] polling producer shutting down", name);
                            break;
                        }
                        if let Err(e) = producer.poll().await {
                            sys_error!("[{}] poll error: {}", name, e);
                        }
                        // 优雅退出：把整个轮询间隔切成 500ms 片段休眠，停机标志置位后最多 500ms 内退出
                        let mut waited_ms = 0u64;
                        while waited_ms < interval * 1000 && !registry_arc.is_shutting_down() {
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                            waited_ms += 500;
                        }
                    }
                });
            } else {
                if let Err(e) = producer.start().await {
                    sys_error!("[{}] start error: {}", name, e);
                }
                sys_info!("[{}] non-polling producer started", name);
            }
        }

        Ok(())
    }

    pub fn consumer_count(&self) -> usize {
        self.consumers
            .read()
            .map(|c| c.values().map(|v| v.len()).sum())
            .unwrap_or(0)
    }

    /// 停机标志是否已置位（worker / producer 循环每轮检查）
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

        let producers = {
            let producers = self
                .producers
                .read()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?;
            producers.clone()
        };
        for producer in producers {
            if let Err(e) = producer.stop().await {
                sys_error!("[{}] producer stop error: {}", producer.name(), e);
            }
        }
        Ok(())
    }

    pub fn producer_count(&self) -> usize {
        self.producers.read().map(|p| p.len()).unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::AtomicUsize;

    #[derive(Clone, Serialize, Deserialize)]
    struct ShutdownTestEvent {
        id: String,
    }

    impl Event for ShutdownTestEvent {
        fn kind(&self) -> EventKind {
            EventKind("test.shutdown")
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
        fn interested_events(&self) -> Vec<EventKind> {
            vec![EventKind("test.shutdown")]
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

    struct StoppableProducer {
        stopped: Arc<AtomicBool>,
    }

    #[async_trait]
    impl Producer for StoppableProducer {
        fn name(&self) -> &str {
            "shutdown_test_producer"
        }
        async fn register(&self, _registry: Arc<Registry>) -> Result<()> {
            Ok(())
        }
        fn poll_interval_secs(&self) -> u64 {
            60
        }
        async fn poll(&self) -> Result<()> {
            Ok(())
        }
        async fn stop(&self) -> Result<()> {
            self.stopped.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    fn new_test_registry() -> Arc<Registry> {
        let registry = Arc::new(Registry::new());
        registry.set_self_ref(registry.clone());
        registry
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
        let stopped = Arc::new(AtomicBool::new(false));
        registry
            .register_producer(Arc::new(StoppableProducer {
                stopped: stopped.clone(),
            }))
            .await
            .unwrap();

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
}
