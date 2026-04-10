//! 内存事件队列实现
//!
//! 纯内存实现，支持：
//! - 按优先级全局排序
//! - 相同 order_key 保证顺序消费
//! - 空 order_key 支持并行消费
//! - ack/nack 完整支持

use std::collections::{HashMap, BinaryHeap, LinkedList};
use std::cell::UnsafeCell;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::event::{Event, EventRef};
use crate::service::dao::event_queue::EventQueueDaoTrait;
use common::constants::RequestContext;

// ==================== 单例 ====================

static EVENT_QUEUE_DAO: OnceLock<Arc<dyn EventQueueDaoTrait>> = OnceLock::new();

/// 获取全局事件队列 DAO 实例
pub fn dao() -> Arc<dyn EventQueueDaoTrait> {
    EVENT_QUEUE_DAO.get().unwrap().clone()
}

/// 初始化全局事件队列 DAO
pub fn init() {
    let dao: Arc<dyn EventQueueDaoTrait> = Arc::new(InMemoryEventQueue::new());
    EVENT_QUEUE_DAO.set(dao).expect("event_queue DAO already initialized");
}

// ==================== 实现 ====================

/// 内存事件队列实现
///
/// 结构设计：
/// - events: 所有未确认事件本体存储（待处理 + 处理中）
/// - queues: 每个 order_key 的等待队列（存储 EventRef），同 order_key 保证顺序
/// - global_heap: 全局优先级堆，存储就绪可消费的 EventRef
/// - in_progress: 当前处理中事件
///
/// 使用 UnsafeCell 实现内部可变性，因为我们已经用 Mutex 保证了独占访问，所以是安全的
#[derive(Debug, Default)]
pub struct InMemoryEventQueue {
    /// 所有未确认事件本体（待处理 + 处理中）
    events: UnsafeCell<HashMap<String, Box<dyn Event>>>,
    /// 每个 order_key 的等待队列
    queues: UnsafeCell<HashMap<String, LinkedList<EventRef>>>,
    /// 全局优先级堆（就绪事件）
    global_heap: UnsafeCell<BinaryHeap<EventRef>>,
    /// 当前处理中事件
    in_progress: UnsafeCell<HashMap<String, (EventRef, String)>>, // (event_ref, order_key)
    /// 互斥锁保护并发访问
    lock: Mutex<()>,
}

// 由于我们只用 UnsafeCell 在 Mutex 保护下进行独占访问，所以 Send/Sync 是安全的
unsafe impl Send for InMemoryEventQueue {}
unsafe impl Sync for InMemoryEventQueue {}

impl InMemoryEventQueue {
    /// 创建新的空内存事件队列
    pub fn new() -> Self {
        Self {
            events: UnsafeCell::new(HashMap::new()),
            queues: UnsafeCell::new(HashMap::new()),
            global_heap: UnsafeCell::new(BinaryHeap::new()),
            in_progress: UnsafeCell::new(HashMap::new()),
            lock: Mutex::new(()),
        }
    }
}

impl EventQueueDaoTrait for InMemoryEventQueue {
    fn enqueue(&self, _ctx: &RequestContext, event: Box<dyn Event>) -> Result<(), AppError> {
        let _guard = self.lock.lock().map_err(|e| AppError::Internal(e.to_string()))?;

        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };

        let event_id = event.id().to_string();
        let order_key = event.order_key().map(|s| s.to_string()).unwrap_or_default();
        let event_ref = event.to_event_ref();

        // 存储事件本体
        events.insert(event_id.clone(), event);

        if order_key.is_empty() {
            // 空 order_key，直接入堆，不需要队列
            global_heap.push(event_ref);
        } else {
            // 非空 order_key，追加到对应队列
            let queue = queues.entry(order_key.clone()).or_default();
            let was_empty = queue.is_empty();
            queue.push_back(event_ref.clone());

            // 如果追加前队列是空的，说明这是该分组第一个事件，插入堆
            if was_empty {
                global_heap.push(event_ref);
            }
        }

        drop(_guard);
        Ok(())
    }

    fn enqueue_batch(&self, ctx: &RequestContext, events: Vec<Box<dyn Event>>) -> Result<(), AppError> {
        for event in events {
            self.enqueue(ctx, event)?;
        }
        Ok(())
    }

    fn dequeue_next(&self, _ctx: &RequestContext) -> Result<Option<Box<dyn Event>>, AppError> {
        let _guard = self.lock.lock().map_err(|e| AppError::Internal(e.to_string()))?;

        let events = unsafe { &mut *self.events.get() };
        let _queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };

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

        // 从处理中移除
        let Some((_event_ref, order_key)) = in_progress.remove(event_id) else {
            drop(_guard);
            return Ok(()); // 已经处理过了
        };

        // 从 events 删除，确认完成
        events.remove(event_id);

        if order_key.is_empty() {
            // 空 order_key，没有队列需要处理
            drop(_guard);
            return Ok(());
        }

        // 非空 order_key，从队列头部移除这个已经处理完的事件
        let Some(queue) = queues.get_mut(&order_key) else {
            drop(_guard);
            return Ok(());
        };

        // 弹出头部（应该就是当前这个事件）
        let Some(_front_ref) = queue.pop_front() else {
            drop(_guard);
            return Ok(());
        };

        // 如果弹出后队列还有元素，新头部入堆
        if let Some(new_front) = queue.front() {
            global_heap.push(new_front.clone());
        }

        // 如果队列空了，移除空队列条目
        if queue.is_empty() {
            queues.remove(&order_key);
        }

        drop(_guard);
        Ok(())
    }

    fn nack(&self, _ctx: &RequestContext, event_id: &str) -> Result<(), AppError> {
        let _guard = self.lock.lock().map_err(|e| AppError::Internal(e.to_string()))?;

        let global_heap = unsafe { &mut *self.global_heap.get() };
        let in_progress = unsafe { &mut *self.in_progress.get() };

        // 从处理中移除
        let Some((event_ref, _order_key)) = in_progress.remove(event_id) else {
            drop(_guard);
            return Ok(());
        };

        // 事件本体还在 events 中（dequeue 只出队处理中标记，不删除）
        // 直接重新插入堆就可以了，等待下次消费
        global_heap.push(event_ref);

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
}
