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

#[derive(Debug, Default)]
pub struct InMemoryEventQueue {
    events: UnsafeCell<HashMap<String, serde_json::Value>>,
    queues: UnsafeCell<HashMap<String, BinaryHeap<EventRef>>>,
    global_heap: UnsafeCell<BinaryHeap<EventRef>>,
    in_progress: UnsafeCell<HashMap<String, (EventRef, String)>>,
    has_active_message: UnsafeCell<HashMap<String, bool>>,
    lock: Mutex<()>,
}

unsafe impl Send for InMemoryEventQueue {}
unsafe impl Sync for InMemoryEventQueue {}

impl InMemoryEventQueue {
    pub fn new() -> Self {
        Self {
            events: UnsafeCell::new(HashMap::new()),
            queues: UnsafeCell::new(HashMap::new()),
            global_heap: UnsafeCell::new(BinaryHeap::new()),
            in_progress: UnsafeCell::new(HashMap::new()),
            has_active_message: UnsafeCell::new(HashMap::new()),
            lock: Mutex::new(()),
        }
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

#[async_trait]
impl EventQueue for InMemoryEventQueue {
    async fn enqueue(&self, _ctx: RequestContext, event: serde_json::Value) -> Result<()> {
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

        let event_ref = EventRef {
            event_id: event_id.clone(),
            order_key: order_key.clone(),
            priority,
            created_at,
        };

        if events.contains_key(&event_id) {
            return Ok(());
        }

        events.insert(event_id.clone(), event);

        if order_key.is_empty() {
            global_heap.push(event_ref);
        } else {
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
        }

        Ok(())
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

        loop {
            let Some(event_ref) = global_heap.pop() else {
                return Ok(None);
            };

            let event_id = &event_ref.event_id;
            let order_key = &event_ref.order_key;

            let Some(event) = events.get(event_id) else {
                continue;
            };

            let cloned_event = event.clone();
            in_progress.insert(event_id.clone(), (event_ref.clone(), order_key.clone()));

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

        let Some((_event_ref, order_key)) = in_progress.remove(event_id) else {
            return Ok(());
        };

        events.remove(event_id);

        if order_key.is_empty() {
            return Ok(());
        }

        let Some(queue) = queues.get_mut(&order_key) else {
            return Ok(());
        };

        if let Some(next_ref) = queue.pop() {
            global_heap.push(next_ref);
            has_active_message.insert(order_key.clone(), true);
        }

        if queue.is_empty() {
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

        let Some((event_ref, order_key)) = in_progress.remove(event_id) else {
            return Ok(());
        };

        global_heap.push(event_ref);
        if !order_key.is_empty() {
            has_active_message.insert(order_key, true);
        }

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

    /// nack 把事件放回可调度堆（重试而非丢弃），且不会把同 `order_key` 的后继锁死
    #[tokio::test]
    async fn nack_requeues_event_and_keeps_successor_blocked() {
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
}
