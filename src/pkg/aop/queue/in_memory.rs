use std::cell::UnsafeCell;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Mutex;

use async_trait::async_trait;
use common::error::{err, Result};
use crate::pkg::RequestContext;

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
        stats.sort_by(|a, b| b.pending_count.cmp(&a.pending_count));
        stats
    }

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
        let _guard = self.lock.lock()
            .map_err(|e| err!(Internal, "failed to acquire event queue lock: {}", e))?;

        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let has_active_message = unsafe { &mut *self.has_active_message.get() };

        let event_id = event.get("message_id")
            .and_then(|v| v.as_str())
            .or_else(|| event.get("id").and_then(|v| v.as_str()))
            .unwrap_or("unknown")
            .to_string();

        let order_key = event.get("order_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let priority = event.get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8;

        let created_at = event.get("created_at")
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

            if was_empty && !has_active_message.get(&order_key).copied().unwrap_or(false) {
                if let Some(top_ref) = queue.pop() {
                    global_heap.push(top_ref);
                    has_active_message.insert(order_key, true);
                }
            }
        }

        Ok(())
    }

    async fn enqueue_batch(&self, ctx: RequestContext, events: Vec<serde_json::Value>) -> Result<()> {
        for event in events {
            self.enqueue(ctx.clone(), event).await?;
        }
        Ok(())
    }

    async fn dequeue_next(&self, _ctx: RequestContext) -> Result<Option<serde_json::Value>> {
        let _guard = self.lock.lock()
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
        let _guard = self.lock.lock()
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
        let _guard = self.lock.lock()
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
            let order_key = event.get("order_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // 应用 order_key 过滤
            if let Some(ref ok) = filter.order_key {
                if &order_key != ok {
                    continue;
                }
            }

            let status = if in_progress.contains_key(event_id) {
                super::EventStatus::Processing
            } else {
                super::EventStatus::Pending
            };

            // 应用 status 过滤
            if let Some(ref s) = filter.status {
                if *s != status {
                    continue;
                }
            }

            let event_kind = event.get("kind")
                .and_then(|v| v.as_str())
                .or_else(|| event.get("message_id").map(|_| "message.created"))
                .unwrap_or("unknown")
                .to_string();

            let priority = event.get("priority")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u8;

            let created_at = event.get("created_at")
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
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // 应用分页
        results.into_iter()
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

        let order_key = event_json.get("order_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let status = if in_progress.contains_key(event_id) {
            super::EventStatus::Processing
        } else {
            super::EventStatus::Pending
        };

        let event_kind = event_json.get("kind")
            .and_then(|v| v.as_str())
            .or_else(|| event_json.get("message_id").map(|_| "message.created"))
            .unwrap_or("unknown")
            .to_string();

        let priority = event_json.get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8;

        let created_at = event_json.get("created_at")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // 脱敏处理：截取前 200 字符
        let payload_preview = {
            let json_str = serde_json::to_string(&event_json).unwrap_or_default();
            if json_str.len() > 200 {
                format!("{}... (truncated, total {} bytes)", &json_str[..200], json_str.len())
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