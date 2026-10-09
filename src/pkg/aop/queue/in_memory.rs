use std::cell::UnsafeCell;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Mutex, MutexGuard};

use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::error::Result;

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
    /// 获取队列内部锁，**poison 自动恢复**。
    ///
    /// 为什么必须恢复：std `Mutex` 在持锁任务 panic 后会**永久 poison**，之后所有
    /// `lock()` 都返回 `Err`。本队列是「一个消费者一个实例」，一旦 poison，该消费者的
    /// 消费链路就此**永久死亡**（awakening 侧表现为 `dequeue error: ... poisoned lock:
    /// another task failed inside` 死循环，用户侧表现为「发消息没响应」）。
    ///
    /// 这里锁保护的是 `HashMap` / `BinaryHeap` 的 insert/remove/pop —— 单次操作要么
    /// 完成、要么不改变结构，**不存在跨多步的半更新不变式**，故 panic 后取回内部 guard
    /// 继续访问是安全的。真正的修复是消灭持锁 panic 源（见 [`Self::get_event`] 的 UTF-8
    /// 安全截断），本恢复是「即便将来又踩到别的 panic，也不至于把整个消费者拖死」的
    /// 第二道防线。
    fn lock_guard(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

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
        let _guard = self.lock_guard();

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
            .unwrap_or_default()
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
        let _guard = self.lock_guard();

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
        let _guard = self.lock_guard();

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
        let _guard = self.lock_guard();

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
        let _guard = self.lock_guard();
        let events = unsafe { &*self.events.get() };
        events.len()
    }

    fn in_progress_count(&self) -> usize {
        let _guard = self.lock_guard();
        let in_progress = unsafe { &*self.in_progress.get() };
        in_progress.len()
    }

    fn recover(&self, _ctx: RequestContext) -> Result<usize> {
        Ok(0)
    }

    fn clear(&self) {
        let _guard = self.lock_guard();
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
        let _guard = self.lock_guard();

        let events = unsafe { &*self.events.get() };
        let in_progress = unsafe { &*self.in_progress.get() };

        super::QueueStats {
            pending_count: events.len().saturating_sub(in_progress.len()),
            in_progress_count: in_progress.len(),
            order_keys: self.collect_order_key_stats(),
            oldest_event_age_secs: self.find_oldest_event_age(),
        }
    }

    fn query_events(&self, filter: super::EventQueryFilter) -> Vec<super::EventSummary> {
        let _guard = self.lock_guard();

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
        let _guard = self.lock_guard();

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
#[path = "in_memory_tests.rs"]
mod tests;
