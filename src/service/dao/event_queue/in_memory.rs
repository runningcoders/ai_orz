//! 内存事件队列实现
//!
//! 纯内存实现，支持：
//! - 按优先级全局排序
//! - 相同 order_key 保证顺序消费
//! - 空 order_key 支持并行消费
//! - ack/nack 完整支持

use std::collections::{HashMap, BinaryHeap};
use std::cell::UnsafeCell;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::event::{Event, EventRef};
use crate::models::message::Message;
use crate::service::dao::event_queue::EventQueueDao;
use crate::pkg::RequestContext;

// ==================== 工厂方法 + 单例 ====================

/// Message topic 的全局单例
static EVENT_QUEUE_MESSAGE: OnceLock<Arc<dyn EventQueueDao<Message>>> = OnceLock::new();

/// 创建一个全新的内存事件队列实例（用于测试或自定义 topic）
///
/// 创建新事件队列实例
pub fn new<E: Event + Clone>() -> Arc<dyn EventQueueDao<E>> {
    Arc::new(EventQueueDaoInMemoryImpl::<E>::new())
}

/// 获取 Message topic 的全局事件队列单例
pub fn message_dao() -> Arc<dyn EventQueueDao<Message>> {
    EVENT_QUEUE_MESSAGE.get().unwrap().clone()
}

/// 初始化 Message topic 全局事件队列
pub fn init_message() {
    let _ = EVENT_QUEUE_MESSAGE.set(new());
}

// ==================== 实现 ====================

/// 内存事件队列实现
///
/// 结构设计：
/// - events: 所有未确认事件本体存储（待处理 + 处理中）
/// - queues: 每个 order_key 的等待堆（存储 EventRef），同 order_key 保证同一时间只处理一个，堆内按优先级排序
/// - global_heap: 全局优先级堆，存储就绪可消费的 EventRef
/// - in_progress: 当前正在处理的事件：event_id -> (event_ref, order_key)
/// - has_waiting_in_global: 记录每个 order_key 是否已经有事件在全局堆等待
///
/// 核心保证：同一个 order_key 全局堆最多只能有一个事件等待，
/// 这样就不会出现多个同 order_key 事件同时在堆里，保证同一时间只处理一个
///
/// 使用 UnsafeCell 实现内部可变性，因为我们已经用 Mutex 保证了独占访问，所以是安全的
#[derive(Debug, Default)]
pub struct EventQueueDaoInMemoryImpl<E: Event + Clone> {
    /// 所有未确认事件本体（待处理 + 处理中）
    events: UnsafeCell<HashMap<String, Box<E>>>,
    /// 每个 order_key 的等待堆（堆内按优先级排序，保证总是取出最高优先级）
    queues: UnsafeCell<HashMap<String, BinaryHeap<EventRef>>>,
    /// 全局优先级堆（就绪事件）
    global_heap: UnsafeCell<BinaryHeap<EventRef>>,
    /// 当前正在处理的事件：event_id -> (event_ref, order_key)
    in_progress: UnsafeCell<HashMap<String, (EventRef, String)>>, // (event_ref, order_key)
    /// 每个 order_key 是否已经有事件在全局堆等待
    has_waiting_in_global: UnsafeCell<HashMap<String, bool>>,
    /// 互斥锁保护并发访问
    lock: Mutex<()>,
}

// 由于我们只用 UnsafeCell 在 Mutex 保护下进行独占访问，所以 Send/Sync 是安全的
unsafe impl<E: Event + Clone + Send> Send for EventQueueDaoInMemoryImpl<E> {}
unsafe impl<E: Event + Clone + Sync> Sync for EventQueueDaoInMemoryImpl<E> {}

impl<E: Event + Clone> EventQueueDaoInMemoryImpl<E> {
    /// 创建新的空内存事件队列
    pub fn new() -> Self {
        Self {
            events: UnsafeCell::new(HashMap::new()),
            queues: UnsafeCell::new(HashMap::new()),
            global_heap: UnsafeCell::new(BinaryHeap::new()),
            in_progress: UnsafeCell::new(HashMap::new()),
            has_waiting_in_global: UnsafeCell::new(HashMap::new()),
            lock: Mutex::new(()),
        }
    }
}

impl<E: Event + Clone> EventQueueDao<E> for EventQueueDaoInMemoryImpl<E> {
    fn enqueue(&self, _ctx: &RequestContext, event: Box<E>) -> Result<(), AppError> {
        let _guard = self.lock.lock().map_err(|e| AppError::Internal(e.to_string()))?;

        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let has_waiting_in_global = unsafe { &mut *self.has_waiting_in_global.get() };

        let event_id = event.id().to_string();
        let order_key = event.order_key().to_string();
        let event_ref = event.to_event_ref();

        // 存储事件本体
        events.insert(event_id.clone(), event);

        if order_key.is_empty() {
            // 空 order_key，直接入全局堆，不需要分组，可并行消费
            global_heap.push(event_ref);
        } else {
            // 非空 order_key，入分组堆，分组堆内按优先级排序
            let queue = queues.entry(order_key.clone()).or_default();
            let was_empty = queue.is_empty();
            queue.push(event_ref.clone());

            // 如果入队前队列是空，说明当前没有等待事件，并且当前还没有事件在全局堆
            // 弹出最高优先级到全局堆，并标记已经有事件在全局堆
            if was_empty && !has_waiting_in_global.get(&order_key).copied().unwrap_or(false) {
                if let Some(top_ref) = queue.pop() {
                    global_heap.push(top_ref);
                    has_waiting_in_global.insert(order_key, true);
                }
            }
        }

        drop(_guard);
        Ok(())
    }

    fn enqueue_batch(&self, ctx: &RequestContext, events: Vec<Box<E>>) -> Result<(), AppError> {
        for event in events {
            self.enqueue(ctx, event)?;
        }
        Ok(())
    }

    fn dequeue_next(&self, _ctx: &RequestContext) -> Result<Option<Box<E>>, AppError> {
        let _guard = self.lock.lock().map_err(|e| AppError::Internal(e.to_string()))?;

        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };
        let has_waiting_in_global = unsafe { &mut *self.has_waiting_in_global.get() };

        loop {
            let Some(event_ref) = global_heap.pop() else {
                drop(_guard);
                return Ok(None);
            };

            let event_id = &event_ref.event_id;
            let order_key = &event_ref.order_key;

            // 检查事件是否还存在（可能已经被处理了）
            let Some(event) = events.get(event_id) else {
                // 事件已经不存在（已经被 ack/nack），跳过这个 ref，继续找下一个
                continue;
            };

            // 从 waiting 标记中移除，因为已经出队到处理中了
            if !order_key.is_empty() {
                has_waiting_in_global.remove(order_key);
                
                // 如果分组堆还有元素， refill 下一个最高优先级到全局堆
                if let Some(queue) = queues.get_mut(order_key) {
                    if let Some(next_ref) = queue.pop() {
                        global_heap.push(next_ref);
                        has_waiting_in_global.insert(order_key.clone(), true);
                    }
                    // 如果分组堆空了，移除空分组（但不需要在这里移除 entry，nack ack 会处理）
                }
            }

            // 克隆一份返回给调用者，原事件保留在 events 直到 ack
            let cloned_event = event.clone();

            // 记录到处理中
            in_progress.insert(event_id.clone(), (event_ref.clone(), order_key.clone()));

            drop(_guard);
            return Ok(Some(cloned_event));
        }
    }

    fn ack(&self, _ctx: &RequestContext, event_id: &str) -> Result<(), AppError> {
        let _guard = self.lock.lock().map_err(|e| AppError::Internal(e.to_string()))?;

        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };
        let has_waiting_in_global = unsafe { &mut *self.has_waiting_in_global.get() };

        // 从处理中移除
        let Some((_event_ref, order_key)) = in_progress.remove(event_id) else {
            drop(_guard);
            return Ok(()); // 已经处理过了
        };

        // 从 events 删除，确认完成
        events.remove(event_id);

        if order_key.is_empty() {
            // 空 order_key，没有分组堆需要处理
            drop(_guard);
            return Ok(());
        }

        // 非空 order_key，如果分组堆还有元素，弹出最高优先级下一个事件到全局堆
        let Some(queue) = queues.get_mut(&order_key) else {
            drop(_guard);
            return Ok(());
        };

        // 如果还有元素，弹出最高优先级到全局堆，并标记已经有事件在全局堆
        if let Some(next_ref) = queue.pop() {
            global_heap.push(next_ref);
            has_waiting_in_global.insert(order_key.clone(), true);
        }

        // 如果分组堆空了，移除空分组条目
        if queue.is_empty() {
            queues.remove(&order_key);
            has_waiting_in_global.remove(&order_key);
        }

        drop(_guard);
        Ok(())
    }

    fn nack(&self, _ctx: &RequestContext, event_id: &str) -> Result<(), AppError> {
        let _guard = self.lock.lock().map_err(|e| AppError::Internal(e.to_string()))?;

        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };
        let has_waiting_in_global = unsafe { &mut *self.has_waiting_in_global.get() };

        // 从处理中移除
        let Some((event_ref, order_key)) = in_progress.remove(event_id) else {
            drop(_guard);
            return Ok(());
        };

        // nack 重新放入全局堆等待下次消费
        global_heap.push(event_ref);
        if !order_key.is_empty() {
            has_waiting_in_global.insert(order_key, true);
        }

        drop(_guard);
        Ok(())
    }

    fn len(&self) -> usize {
        let _guard = self.lock.lock().ok();
        let events = unsafe { &*self.events.get() };
        events.len()
    }

    fn is_empty(&self) -> bool {
        let _guard = self.lock.lock().ok();
        let events = unsafe { &*self.events.get() };
        events.is_empty()
    }

    fn in_progress_count(&self) -> usize {
        let _guard = self.lock.lock().ok();
        let in_progress = unsafe { &*self.in_progress.get() };
        in_progress.len()
    }

    fn recover(&self, _ctx: &RequestContext) -> Result<usize, AppError> {
        // 内存版本不需要从持久化恢复，恢复由上层调用者结合数据库完成
        Ok(0)
    }

    fn clear(&self) {
        let _guard = self.lock.lock().ok();
        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };
        let has_waiting_in_global = unsafe { &mut *self.has_waiting_in_global.get() };

        events.clear();
        queues.clear();
        global_heap.clear();
        in_progress.clear();
        has_waiting_in_global.clear();
    }
}