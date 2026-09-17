use std::cell::UnsafeCell;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Mutex;

use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::error::{Result, err};

use super::EventQueue;

#[derive(Debug, Clone)]
struct EventRef {
    event_id: String,
    order_key: String,
    priority: u8,
    created_at: i64,
    /// 本事件被消费的**累计次数**：入队即 1，每次 `nack` 自增（`ack` 后随事件消亡，无需持久化）
    attempt: u32,
    /// 最早可出队时刻（epoch 毫秒；`0` = 立即可取）。仅 `nack` 重投时设置（per-event 指数退避）。
    /// 本事件是否走了「同 key 门闩」
    ///
    /// 由入队方法决定：`enqueue`（未下沉前 = 订阅声明了 `ordered`）为 true，
    /// `enqueue_ungated` 为 false。
    ///
    /// ⚠️ **为什么必须记住**：`ack`/`nack` 只能看到「这个事件的 order_key 是什么」，
    /// 看不到它进的是哪条路。而**一个消费者只有一个队列**，`order_key` 又是事件侧
    /// 的值 —— 同一消费者下不同订阅的事件完全可能带同一个 key。若不区分，
    /// ungated 事件的 ack 会去 pop gated 队列的后继并提前放行它（`ordered` 的
    /// 串行保证被悄悄绕过），ungated 事件的 nack 则会给该 key 打上「门闩占用」
    /// 却再无 gated 事件能解开它（该 key 永久饥饿）。
    gated: bool,
    not_before: i64,
}

impl PartialEq for EventRef {
    fn eq(&self, other: &Self) -> bool {
        self.event_id == other.event_id
    }
}

impl Eq for EventRef {}

impl PartialOrd for EventRef {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EventRef {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.created_at.cmp(&self.created_at))
    }
}

#[derive(Debug)]
pub struct InMemoryEventQueue {
    events: UnsafeCell<HashMap<String, serde_json::Value>>,
    queues: UnsafeCell<HashMap<String, BinaryHeap<EventRef>>>,
    global_heap: UnsafeCell<BinaryHeap<EventRef>>,
    in_progress: UnsafeCell<HashMap<String, (EventRef, String)>>,
    has_active_message: UnsafeCell<HashMap<String, bool>>,
    lock: Mutex<()>,
    /// 重试退避参数（生产默认 1s 起、60s 封顶；测试注入毫秒级以保持用例速度）
    backoff_base_ms: i64,
    backoff_max_ms: i64,
}

unsafe impl Send for InMemoryEventQueue {}
unsafe impl Sync for InMemoryEventQueue {}

impl Default for InMemoryEventQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEventQueue {
    pub fn new() -> Self {
        Self::with_backoff_policy(RETRY_BACKOFF_BASE_MS, RETRY_BACKOFF_MAX_MS)
    }

    /// 以自定义退避参数构造（仅测试用：毫秒级退避换取用例速度）
    pub fn with_backoff_policy(base_ms: i64, max_ms: i64) -> Self {
        Self {
            events: UnsafeCell::new(HashMap::new()),
            queues: UnsafeCell::new(HashMap::new()),
            global_heap: UnsafeCell::new(BinaryHeap::new()),
            in_progress: UnsafeCell::new(HashMap::new()),
            has_active_message: UnsafeCell::new(HashMap::new()),
            lock: Mutex::new(()),
            backoff_base_ms: base_ms,
            backoff_max_ms: max_ms,
        }
    }

    /// 入队本体。`gated = true` 时事件带非空 `order_key` 即进「同 key FIFO 门闩队列」
    /// （首条上堆、其余在 key 队列里等前一条 ack）；`gated = false` 时无视
    /// `order_key` 直进堆，按 `(priority, created_at)` 排序 —— 供**未声明
    /// `ordered`** 的订阅走（门闩是订阅者的 opt-in，见设计稿 §4.1）。
    async fn do_enqueue(&self, event: serde_json::Value, gated: bool) -> Result<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|e| err!(Internal, "failed to acquire event queue lock: {}", e))?;

        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let has_active_message = unsafe { &mut *self.has_active_message.get() };

        let event_id = event
            .get("event_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let order_key = event
            .get("order_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let priority = event.get("priority").and_then(|v| v.as_u64()).unwrap_or(0) as u8;

        let created_at = event
            .get("created_at")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // 只有在真正进门闩时 `gated` 才成立：空 key 的事件即使在 gated 订阅下
        // 也是直进堆（没有 key 就没有同串行的对象）。
        let gate = gated && !order_key.is_empty();

        let event_ref = EventRef {
            event_id: event_id.clone(),
            order_key: order_key.clone(),
            priority,
            created_at,
            // 首次取出消费即 attempt == 1（`nack` 时自增）
            attempt: 1,
            not_before: 0,
            gated: gate,
        };

        if events.contains_key(&event_id) {
            return Ok(());
        }

        events.insert(event_id.clone(), event);

        if gate {
            let queue = queues.entry(order_key.clone()).or_default();
            let was_empty = queue.is_empty();
            queue.push(event_ref.clone());

            if was_empty
                && !has_active_message.get(&order_key).copied().unwrap_or(false)
                && let Some(top_ref) = queue.pop()
            {
                global_heap.push(top_ref);
                has_active_message.insert(order_key, true);
            }
        } else {
            global_heap.push(event_ref);
        }

        Ok(())
    }

    /// 收集各 order_key 的待处理数量统计。
    /// 注意：调用前必须持有 self.lock，否则存在数据竞争。
    fn collect_order_key_stats(&self) -> Vec<super::OrderKeyStats> {
        let queues = unsafe { &*self.queues.get() };

        let mut stats = Vec::new();
        for (order_key, queue) in queues.iter() {
            let pending = queue.len();
            if pending > 0 {
                stats.push(super::OrderKeyStats {
                    order_key: order_key.clone(),
                    pending_count: pending,
                });
            }
        }
        stats.sort_by_key(|x| std::cmp::Reverse(x.pending_count));
        stats
    }

    /// 查找最老事件的年龄（秒）。
    /// 注意：调用前必须持有 self.lock，否则存在数据竞争。
    fn find_oldest_event_age(&self) -> Option<u64> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let events = unsafe { &*self.events.get() };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        events
            .values()
            .filter_map(|e| e.get("created_at").and_then(|v| v.as_i64()))
            .min()
            .map(|oldest| (now - oldest) as u64)
    }
}

/// per-event 重试退避：第 `attempt` 次消费失败重投前的静默时长（毫秒）
///
/// 指数递增：attempt 2→1s、3→2s、4→4s … 封顶 `max_ms`。
/// `attempt` 由 `nack` 自增后传入（首次重投 attempt == 2 → base）。
/// 次数上限**不在队列层**：是否继续重试由消费者的 `decide_retry` 回答
/// （默认：永久性错误码 → 放弃；累计 8 次 → 放弃）。
const RETRY_BACKOFF_BASE_MS: i64 = 1_000;
const RETRY_BACKOFF_MAX_MS: i64 = 60_000;

fn retry_backoff_ms(base_ms: i64, max_ms: i64, attempt: u32) -> i64 {
    let shift = attempt.saturating_sub(2).min(6);
    (base_ms << shift).min(max_ms)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[async_trait]
impl EventQueue for InMemoryEventQueue {
    async fn enqueue(&self, _ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        self.do_enqueue(event, /* gated */ true).await
    }

    async fn enqueue_ungated(&self, _ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        self.do_enqueue(event, /* gated */ false).await
    }

    async fn enqueue_batch(
        &self,
        ctx: RequestContext,
        events: Vec<serde_json::Value>,
    ) -> Result<()> {
        for event in events {
            self.enqueue(ctx.clone(), event).await?;
        }
        Ok(())
    }

    async fn dequeue_next(&self, _ctx: RequestContext) -> Result<Option<serde_json::Value>> {
        let _guard = self
            .lock
            .lock()
            .map_err(|e| err!(Internal, "failed to acquire event queue lock: {}", e))?;

        let events = unsafe { &mut *self.events.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };

        let now = now_ms();
        // 退避中（not_before 未到）的事件先摘出来，取完再放回——
        // 它们不阻塞堆里已到期的后继事件。
        let mut deferred: Vec<EventRef> = Vec::new();

        loop {
            let Some(event_ref) = global_heap.pop() else {
                global_heap.extend(deferred);
                return Ok(None);
            };

            if event_ref.not_before > now {
                deferred.push(event_ref);
                continue;
            }

            let event_id = event_ref.event_id.clone();
            let order_key = event_ref.order_key.clone();

            let Some(event) = events.get(&event_id) else {
                // 事件体已消失（ack 竞态窗口）：同旧实现——丢弃该 ref，继续扫堆
                continue;
            };

            // 把「第几次消费」注入封套顶层，供消费侧 `AopEventMeta::from_json` 读出：
            // `Producer::on_failed` 据此实现「退避 N 次后放弃」。
            // 只注入返回的副本，不改 `events` 里那份（监控页展示的是原始事件）。
            let mut cloned_event = event.clone();
            if let Some(obj) = cloned_event.as_object_mut() {
                obj.insert(
                    "attempt".to_string(),
                    serde_json::Value::from(event_ref.attempt),
                );
            }
            in_progress.insert(event_id, (event_ref, order_key));

            global_heap.extend(deferred);
            return Ok(Some(cloned_event));
        }
    }

    async fn ack(&self, _ctx: RequestContext, event_id: &str) -> Result<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|e| err!(Internal, "failed to acquire event queue lock: {}", e))?;

        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };
        let has_active_message = unsafe { &mut *self.has_active_message.get() };

        let Some((event_ref, _order_key)) = in_progress.remove(event_id) else {
            return Ok(());
        };

        events.remove(event_id);

        // ⚠️ 只有走门闩进来的事件碰门闩状态；ungated 事件即使带着同一个
        // `order_key`，也与相邻小贴士/latch 无关（详见 `EventRef::gated`）。
        if !event_ref.gated {
            return Ok(());
        }
        let order_key = event_ref.order_key.clone();

        let Some(queue) = queues.get_mut(&order_key) else {
            return Ok(());
        };

        // ⚠️ 两段必须互斥：只要还 pop 出了后继（它已上堆、等待被消费），
        // `has_active_message` 就**必须**保持 true —— 否则下一个同 key 事件入队时
        // 会判定「门闩空闲」而直接上堆，与上一个后继同时在堆里 → 并发 > 1 时同 key
        // 被并行消费，`order_key` 串行门闩静默失效（FIFO 也可能乱序）。
        // 只有 pop 返回 None（该 key 确实无后继）才清理状态。
        if let Some(next_ref) = queue.pop() {
            global_heap.push(next_ref);
            has_active_message.insert(order_key.clone(), true);
        } else {
            queues.remove(&order_key);
            has_active_message.remove(&order_key);
        }

        Ok(())
    }

    async fn nack(&self, _ctx: RequestContext, event_id: &str) -> Result<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|e| err!(Internal, "failed to acquire event queue lock: {}", e))?;

        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };
        let has_active_message = unsafe { &mut *self.has_active_message.get() };

        let Some((mut event_ref, order_key)) = in_progress.remove(event_id) else {
            return Ok(());
        };

        // 重投：累计消费次数自增 → 下次出队时封套里的 `attempt` 即新值
        event_ref.attempt = event_ref.attempt.saturating_add(1);
        // per-event 指数退避：第 n 次重试前先静默 `retry_backoff_ms(attempt)`，
        // 期间 `dequeue_next` 跳过它（不阻塞同堆其他事件）。
        event_ref.not_before = now_ms()
            + retry_backoff_ms(self.backoff_base_ms, self.backoff_max_ms, event_ref.attempt);

        // gated 事件重投后仍然占着门闩 —— 这里是显式重申（幂等），不是状态迁移：
        // 真正的释放只在 `ack` 里发生。
        if event_ref.gated {
            has_active_message.insert(order_key, true);
        }

        global_heap.push(event_ref);
        // ⚠️ 同一 key 上的 ungated 事件**不写**这个标记：它不是从 key 队列出来的，
        // 没有一个 gated 的 ack 能为它收尾 → 一旦写上就再没人解开，该 key 的
        // gated 后继永远不会被放行（永久饥饿）。
        Ok(())
    }

    fn len(&self) -> usize {
        let _guard = self.lock.lock().ok();
        let events = unsafe { &*self.events.get() };
        events.len()
    }

    fn in_progress_count(&self) -> usize {
        let _guard = self.lock.lock().ok();
        let in_progress = unsafe { &*self.in_progress.get() };
        in_progress.len()
    }

    fn recover(&self, _ctx: RequestContext) -> Result<usize> {
        Ok(0)
    }

    fn clear(&self) {
        let _guard = self.lock.lock().ok();
        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };
        let has_active_message = unsafe { &mut *self.has_active_message.get() };

        events.clear();
        queues.clear();
        global_heap.clear();
        in_progress.clear();
        has_active_message.clear();
    }

    fn stats(&self) -> super::QueueStats {
        let _guard = self.lock.lock().ok();

        let events = unsafe { &*self.events.get() };
        let in_progress = unsafe { &*self.in_progress.get() };

        super::QueueStats {
            pending_count: events.len() - in_progress.len(),
            in_progress_count: in_progress.len(),
            order_keys: self.collect_order_key_stats(),
            oldest_event_age_secs: self.find_oldest_event_age(),
        }
    }

    fn query_events(&self, filter: super::EventQueryFilter) -> Vec<super::EventSummary> {
        let _guard = self.lock.lock().ok();

        let events = unsafe { &*self.events.get() };
        let in_progress = unsafe { &*self.in_progress.get() };

        let mut results: Vec<super::EventSummary> = Vec::new();

        // 收集所有事件
        for (event_id, event) in events.iter() {
            let order_key = event
                .get("order_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // 应用 order_key 过滤
            if let Some(ref ok) = filter.order_key
                && &order_key != ok
            {
                continue;
            }

            let status = if in_progress.contains_key(event_id) {
                super::EventStatus::Processing
            } else {
                super::EventStatus::Pending
            };

            // 应用 status 过滤
            if let Some(ref s) = filter.status
                && *s != status
            {
                continue;
            }

            let event_kind = event
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let priority = event.get("priority").and_then(|v| v.as_u64()).unwrap_or(0) as u8;

            let created_at = event
                .get("created_at")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            results.push(super::EventSummary {
                event_id: event_id.clone(),
                event_kind,
                order_key,
                priority,
                created_at,
                status,
            });
        }

        // 按 created_at 降序排序（最新的在前）
        results.sort_by_key(|x| std::cmp::Reverse(x.created_at));

        // 应用分页
        results
            .into_iter()
            .skip(filter.offset)
            .take(filter.limit)
            .collect()
    }

    fn get_event(&self, event_id: &str) -> Option<super::EventDetail> {
        let _guard = self.lock.lock().ok();

        let events = unsafe { &*self.events.get() };
        let in_progress = unsafe { &*self.in_progress.get() };

        let event = events.get(event_id)?;
        let event_json = event.clone();

        let order_key = event_json
            .get("order_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let status = if in_progress.contains_key(event_id) {
            super::EventStatus::Processing
        } else {
            super::EventStatus::Pending
        };

        let event_kind = event_json
            .get("kind")
            .and_then(|v| v.as_str())
            .or_else(|| event_json.get("message_id").map(|_| "message.created"))
            .unwrap_or("unknown")
            .to_string();

        let priority = event_json
            .get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8;

        let created_at = event_json
            .get("created_at")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // 脱敏处理：截取前 200 字符
        let payload_preview = {
            let json_str = serde_json::to_string(&event_json).unwrap_or_default();
            if json_str.len() > 200 {
                format!(
                    "{}... (truncated, total {} bytes)",
                    &json_str[..200],
                    json_str.len()
                )
            } else {
                json_str
            }
        };

        Some(super::EventDetail {
            summary: super::EventSummary {
                event_id: event_id.to_string(),
                event_kind,
                order_key,
                priority,
                created_at,
                status,
            },
            payload_preview,
        })
    }
}

#[cfg(test)]
mod tests {
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
}
