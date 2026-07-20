# AOP 生产-消费事件中心实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建通用 AOP 生产-消费事件中心框架，将现有 event_queue dao 彻底收敛到 AOP 框架中，统一所有内部事件分发入口。

**Architecture:**
- Producer-Consumer AOP 模式，Registry 作为核心分发器
- Queue 作为底层可靠存储，复用现有 InMemoryEventQueue 实现
- 删除 service/dao/event_queue/，全部迁移到 pkg/aop/queue/
- MessageDomain 从「直接入队」改为「发布事件」
- Consumer 从「直接取队」改为「从 registry 取」

**Tech Stack:** Rust + async-trait + tokio + serde

---

## 文件结构（最终态）

```
pkg/aop/                              # 统一的 AOP 事件中心
├── mod.rs                            # 入口，re-export 核心 API
│
├── core/                             # 核心抽象
│   ├── mod.rs
│   ├── event.rs                      # Event trait + EventKind
│   ├── producer.rs                   # Producer trait
│   ├── consumer.rs                   # Consumer trait + ConsumeMode
│   └── registry.rs                   # Registry 分发器
│
├── queue/                            # 底层队列（框架内部使用）
│   ├── mod.rs                        # EventQueue trait + 工厂方法
│   ├── in_memory.rs                  # 内存实现（从 dao 迁移而来）
│   └── in_memory_test.rs             # 测试（从 dao 迁移而来）
│
└── impl/                             # 业务事件/消费者实现
    ├── mod.rs
    └── message/
        ├── mod.rs
        ├── events.rs                 # MessageCreatedEvent 等
        └── consumers.rs              # AgentAwakeningConsumer 等

service/dao/event_queue/              → ❌ 删除（全部迁移到 pkg/aop/queue/）
```

---

## 受影响文件清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `service/dao/event_queue/mod.rs` | 删除 | 迁移到 pkg/aop/queue/mod.rs |
| `service/dao/event_queue/in_memory.rs` | 删除 | 迁移到 pkg/aop/queue/in_memory.rs |
| `service/dao/event_queue/in_memory_test.rs` | 删除 | 迁移到 pkg/aop/queue/in_memory_test.rs |
| `service/dao/mod.rs` | 修改 | 移除 event_queue 模块引用 |
| `service/dal/message.rs` | 修改 | 从 event_queue::dao() 改为 aop::registry() |
| `service/domain/message/delivery.rs` | 修改 | send_to_agent 从 enqueue 改为 publish |
| `service/domain/message/delivery_test.rs` | 修改 | 测试改造 |
| `consumer/scheduler.rs` | 修改 | 初始化从 init_message 改为注册 consumer |
| `scheduler/mod.rs` | 修改 | cron_trigger 队列迁移 |
| `handlers/a2a/integration_test.rs` | 修改 | 初始化方式 |
| `handlers/a2a/send_task.rs` | 修改 | 如有入队逻辑也改 |

---

## Task 1: 创建 AOP 核心抽象

**Files:**
- Create: `src/pkg/aop/mod.rs`
- Create: `src/pkg/aop/core/mod.rs`
- Create: `src/pkg/aop/core/event.rs`
- Create: `src/pkg/aop/core/producer.rs`
- Create: `src/pkg/aop/core/consumer.rs`
- Modify: `src/pkg/mod.rs`（注册 aop 模块）

- [ ] **Step 1: 创建 src/pkg/aop/mod.rs**

```rust
//! AOP 生产-消费事件中心
//!
//! 统一事件分发框架，核心概念：
//! - Event: 事件（携带数据）
//! - Producer: 生产者（发布事件）
//! - Consumer: 消费者（处理事件）
//! - Registry: 注册中心（分发事件）
//! - Queue: 底层队列（异步消费存储）
//!
//! 使用方式：
//! ```ignore
//! // 发布事件
//! aop::publish(MyEvent { ... }).await;
//!
//! // 注册消费者
//! aop::registry().register_consumer(Arc::new(MyConsumer)).unwrap();
//!
//! // 异步消费端取任务
//! let event: MyEvent = aop::registry().dequeue_for("my_consumer").await?;
//! ```

pub mod core;
pub mod queue;
pub mod impl as impls;

// 重导出核心 API
pub use core::{Event, EventKind, Producer, Consumer, ConsumeMode, Registry};
pub use queue::EventQueue;

use once_cell::sync::Lazy;

/// 全局 Registry 单例
static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);

/// 获取全局 Registry
pub fn registry() -> &'static Registry {
    &REGISTRY
}

/// 发布事件（便捷方法）
pub async fn publish<E: Event>(event: E) {
    REGISTRY.publish(event).await
}

/// 初始化所有消费者和生产者（启动时调用）
pub async fn init_all() -> common::error::Result<()> {
    use impls::message::AgentAwakeningConsumer;
    use std::sync::Arc;

    // 注册消息相关消费者
    REGISTRY.register_consumer(Arc::new(AgentAwakeningConsumer))?;

    // TODO: 注册其他消费者...

    Ok(())
}
```

- [ ] **Step 2: 创建 src/pkg/aop/core/mod.rs**

```rust
mod event;
mod producer;
mod consumer;
mod registry;

pub use event::{Event, EventKind};
pub use producer::Producer;
pub use consumer::{Consumer, ConsumeMode};
pub use registry::Registry;
```

- [ ] **Step 3: 创建 src/pkg/aop/core/event.rs**

```rust
use serde::{de::DeserializeOwned, Serialize};

/// 事件类型标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventKind(pub &'static str);

impl EventKind {
    // === 消息系统 ===
    pub const MESSAGE_CREATED: Self = Self("message.created");
    pub const MESSAGE_DELIVERED: Self = Self("message.delivered");

    // === 项目 ===
    pub const PROJECT_UPDATED: Self = Self("project.updated");
    pub const PROJECT_STATUS_CHANGED: Self = Self("project.status_changed");

    // === Agent ===
    pub const AGENT_AWAKENED: Self = Self("agent.awakened");

    // === 工具调用 ===
    pub const TOOL_CALL_COMPLETED: Self = Self("tool_call.completed");

    // === 产物 ===
    pub const ARTIFACT_GENERATED: Self = Self("artifact.generated");

    // === 外部消息入站 ===
    pub const EXTERNAL_MESSAGE_INBOUND: Self = Self("external.message.inbound");

    // === 定时触发器 ===
    pub const CRON_TRIGGER: Self = Self("cron.trigger");
}

/// 事件 trait
///
/// 所有事件必须实现此 trait。事件是不可变的数据载体，
/// 在生产者和消费者之间传递。
pub trait Event: Send + Sync + Clone + Serialize + DeserializeOwned + 'static {
    /// 事件类型
    fn kind(&self) -> EventKind;

    /// 事件 ID（用于追踪和去重）
    fn id(&self) -> &str;

    /// 顺序 key
    /// - 空字符串表示可并行消费
    /// - 非空表示同 key 严格顺序消费（同一时间只处理一个）
    fn order_key(&self) -> &str {
        ""
    }

    /// 优先级（默认 0，越大越优先）
    fn priority(&self) -> u8 {
        0
    }

    /// 创建时间戳（毫秒）
    fn created_at(&self) -> i64 {
        0
    }
}
```

- [ ] **Step 4: 创建 src/pkg/aop/core/producer.rs**

```rust
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

use super::Registry;

/// 生产者 trait
///
/// 负责将外部/内部变化转换为事件，发布到 Registry。
///
/// 实现分类：
/// - 内部生产者：在业务代码中直接调用 aop::publish()（无需实现此 trait）
/// - 外部生产者：监听外部渠道（飞书、Webhook 等），需实现此 trait 并注册
#[async_trait]
pub trait Producer: Send + Sync {
    /// 生产者名称（用于日志和监控）
    fn name(&self) -> &str;

    /// 注册到 Registry（框架调用，可保存 registry 引用用于发布）
    async fn register(&self, registry: Arc<Registry>) -> Result<()>;

    /// 启动生产（如启动外部渠道监听）
    async fn start(&self) -> Result<()> {
        Ok(())
    }

    /// 停止生产
    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}
```

- [ ] **Step 5: 创建 src/pkg/aop/core/consumer.rs**

```rust
use async_trait::async_trait;
use common::error::Result;

use super::{Event, EventKind};

/// 消费模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumeMode {
    /// 同步回调（实时推送，适合轻量操作）
    /// 如：SSE 推送、日志记录、指标统计
    Sync,

    /// 异步队列（可靠消费，适合重任务）
    /// 如：Agent 唤醒、工具执行、发送通知
    Async,
}

/// 消费者 trait
///
/// 负责处理特定类型的事件。
///
/// 实现分类：
/// - 同步消费者：publish 时直接调用 on_event，轻量操作
/// - 异步消费者：publish 时入队，由 worker 异步调用 on_event
#[async_trait]
pub trait Consumer: Send + Sync {
    /// 消费者名称（用于队列标识和日志，必须唯一）
    fn name(&self) -> &str;

    /// 关心的事件类型列表
    fn interested_events(&self) -> Vec<EventKind>;

    /// 消费模式（默认同步）
    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    /// 处理事件
    async fn on_event(&self, event: &dyn Event) -> Result<()>;
}
```

- [ ] **Step 6: 修改 src/pkg/mod.rs，注册 aop 模块**

在文件中添加：
```rust
pub mod aop;
```

- [ ] **Step 7: 运行 cargo check 验证编译**

Run: `cargo check`
Expected: 编译通过（可能有 unused 警告，正常）

- [ ] **Step 8: 提交**

```bash
git add src/pkg/aop/ src/pkg/mod.rs
git commit -m "feat(aop): 添加核心抽象（Event/Producer/Consumer/Registry trait）"
```

---

## Task 2: 迁移 Queue 到 AOP 框架

**Files:**
- Create: `src/pkg/aop/queue/mod.rs`
- Create: `src/pkg/aop/queue/in_memory.rs`
- Create: `src/pkg/aop/queue/in_memory_test.rs`
- Delete: `src/service/dao/event_queue/mod.rs`
- Delete: `src/service/dao/event_queue/in_memory.rs`
- Delete: `src/service/dao/event_queue/in_memory_test.rs`
- Modify: `src/service/dao/mod.rs`

- [ ] **Step 1: 创建 src/pkg/aop/queue/mod.rs**

```rust
//! EventQueue 队列抽象
//!
//! 事件队列的统一抽象，用于异步消费者的可靠存储。
//! 支持不同实现替换（内存、Redis、SQLite 等）。

mod in_memory;

use async_trait::async_trait;
use common::error::Result;
use crate::pkg::RequestContext;
use crate::pkg::aop::Event;

pub use in_memory::InMemoryEventQueue;

/// 事件队列 trait
#[async_trait]
pub trait EventQueue: Send + Sync + std::fmt::Debug {
    /// 入队一个事件
    async fn enqueue<E: Event>(&self, ctx: RequestContext, event: E) -> Result<()>;

    /// 批量入队多个事件
    async fn enqueue_batch<E: Event>(&self, ctx: RequestContext, events: Vec<E>) -> Result<()>;

    /// 获取下一个待处理事件
    /// 返回 None 表示队列为空
    /// 获取后事件进入 "处理中" 状态，需要调用 ack 确认完成
    async fn dequeue_next<E: Event>(&self, ctx: RequestContext) -> Result<Option<E>>;

    /// 确认事件处理完成，从队列中移除
    async fn ack(&self, ctx: RequestContext, event_id: &str) -> Result<()>;

    /// 标记事件处理失败，重新放回队列等待重试
    async fn nack(&self, ctx: RequestContext, event_id: &str) -> Result<()>;

    /// 获取当前队列总长度（包含待处理 + 处理中）
    fn len(&self) -> usize;

    /// 判断队列是否为空
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 获取处理中事件数量
    fn in_progress_count(&self) -> usize;

    /// 恢复启动：从持久化层恢复未完成事件重新入队
    /// 返回恢复的事件数量
    fn recover(&self, ctx: RequestContext) -> Result<usize>;

    /// 清空所有队列（测试用）
    fn clear(&self);
}

/// 创建一个新的内存队列实例
pub fn new_in_memory<E: Event + Clone>() -> std::sync::Arc<dyn EventQueue> {
    std::sync::Arc::new(InMemoryEventQueue::<E>::new())
}
```

- [ ] **Step 2: 创建 src/pkg/aop/queue/in_memory.rs**

直接从 `service/dao/event_queue/in_memory.rs` 迁移，做以下调整：

1. 把 `EventQueueDao<E>` trait 名改为符合 `EventQueue` trait 的方法签名
2. 把 `EventQueueDaoInMemoryImpl<E>` 改名为 `InMemoryEventQueue<E>`
3. 移除对 `crate::models::event::Event` 的依赖，改用 `crate::pkg::aop::Event`
4. 移除对 `crate::models::event::EventRef` 的依赖，在本文件内定义 EventRef
5. 移除 Message / CronTrigger 的具体单例（message_dao / cron_trigger_dao / init_message / init_cron_trigger）
6. 保留 `new()` 工厂方法
7. 同步方法改为 async 方法

完整代码（关键调整点已标注）：

```rust
//! 内存事件队列实现
//! 纯内存实现，支持：
//! - 按优先级全局排序
//! - 相同 order_key 保证顺序消费
//! - 空 order_key 支持并行消费
//! - ack/nack 完整支持

use std::cell::UnsafeCell;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Mutex;

use async_trait::async_trait;
use common::error::{err, Result};
use serde::{de::DeserializeOwned, Serialize};
use crate::pkg::RequestContext;
use crate::pkg::aop::Event;

use super::EventQueue;

// ==================== EventRef（内部结构） ====================

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

// ==================== 实现 ====================

/// 内存事件队列实现
#[derive(Debug, Default)]
pub struct InMemoryEventQueue<E: Event + Clone> {
    events: UnsafeCell<HashMap<String, E>>,
    queues: UnsafeCell<HashMap<String, BinaryHeap<EventRef>>>,
    global_heap: UnsafeCell<BinaryHeap<EventRef>>,
    in_progress: UnsafeCell<HashMap<String, (EventRef, String)>>,
    has_active_message: UnsafeCell<HashMap<String, bool>>,
    lock: Mutex<()>,
    _phantom: std::marker::PhantomData<E>,
}

unsafe impl<E: Event + Clone + Send> Send for InMemoryEventQueue<E> {}
unsafe impl<E: Event + Clone + Sync> Sync for InMemoryEventQueue<E> {}

impl<E: Event + Clone> InMemoryEventQueue<E> {
    pub fn new() -> Self {
        Self {
            events: UnsafeCell::new(HashMap::new()),
            queues: UnsafeCell::new(HashMap::new()),
            global_heap: UnsafeCell::new(BinaryHeap::new()),
            in_progress: UnsafeCell::new(HashMap::new()),
            has_active_message: UnsafeCell::new(HashMap::new()),
            lock: Mutex::new(()),
            _phantom: std::marker::PhantomData,
        }
    }

    fn to_event_ref(&self, event: &E) -> EventRef {
        EventRef {
            event_id: event.id().to_string(),
            order_key: event.order_key().to_string(),
            priority: event.priority(),
            created_at: event.created_at(),
        }
    }
}

#[async_trait]
impl<E: Event + Clone + Serialize + DeserializeOwned> EventQueue for InMemoryEventQueue<E> {
    async fn enqueue<EV: Event>(&self, _ctx: RequestContext, event: EV) -> Result<()> {
        // 因为 EventQueue trait 是泛型方法，但我们的实现是具体 E，
        // 所以这里通过序列化中转
        let payload = serde_json::to_vec(&event)?;
        let typed_event: E = serde_json::from_slice(&payload)?;

        let _guard = self.lock.lock()
            .map_err(|e| err!(Internal, "failed to acquire event queue lock: {}", e))?;

        let events = unsafe { &mut *self.events.get() };
        let queues = unsafe { &mut *self.queues.get() };
        let global_heap = unsafe { &mut *self.global_heap.get() };
        let has_active_message = unsafe { &mut *self.has_active_message.get() };

        let event_id = typed_event.id().to_string();
        let order_key = typed_event.order_key().to_string();
        let event_ref = self.to_event_ref(&typed_event);

        if events.contains_key(&event_id) {
            return Ok(());
        }

        events.insert(event_id.clone(), typed_event);

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

    async fn enqueue_batch<EV: Event>(&self, ctx: RequestContext, events: Vec<EV>) -> Result<()> {
        for event in events {
            self.enqueue(ctx.clone(), event).await?;
        }
        Ok(())
    }

    async fn dequeue_next<EV: Event>(&self, _ctx: RequestContext) -> Result<Option<EV>> {
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

            // 序列化中转
            let payload = serde_json::to_vec(&cloned_event)?;
            let result: EV = serde_json::from_slice(&payload)?;

            return Ok(Some(result));
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
}
```

- [ ] **Step 3: 创建 src/pkg/aop/queue/in_memory_test.rs**

从 `service/dao/event_queue/in_memory_test.rs` 迁移，调整：
1. 导入路径改为 `crate::pkg::aop::Event`
2. `EventQueueDaoInMemoryImpl` 改为 `InMemoryEventQueue`
3. 移除 `Box<E>` 包装，直接用 `E`
4. 同步方法调用改为 `.await`

关键测试代码结构保持不变，共 10 个测试用例：
- test_event_queue_empty
- test_single_event_enqueue_dequeue_ack
- test_priority_ordering
- test_same_time_priority_ordering
- test_same_priority_time_ordering
- test_same_order_key_sequential
- test_nack_retry
- test_same_order_key_while_processing
- test_order_key_nack_strict_ordering
- test_batch_enqueue
- test_mixed_order_groups

> 注意：因为 Event trait 方法签名变化（去掉了 clone_box/as_any/into_any 等），
> 测试中的 TestEvent 实现也要调整为直接实现新的 Event trait。

- [ ] **Step 4: 修改 src/service/dao/mod.rs，移除 event_queue 模块**

删除 `pub mod event_queue;` 以及 `event_queue::init_message()` / `init_cron_trigger()` 等调用。
（具体修改需要看当前 mod.rs 内容，执行时再确认）

- [ ] **Step 5: 删除 service/dao/event_queue/ 目录下三个文件**

```bash
rm src/service/dao/event_queue/mod.rs
rm src/service/dao/event_queue/in_memory.rs
rm src/service/dao/event_queue/in_memory_test.rs
```

- [ ] **Step 6: 运行 cargo check 修复编译错误**

Run: `cargo check`
Expected: 会有大量编译错误（因为还没迁移引用点），这是预期的，下一步处理。

- [ ] **Step 7: 提交 Queue 迁移**

```bash
git add src/pkg/aop/queue/
git rm src/service/dao/event_queue/*.rs
git add src/service/dao/mod.rs
git commit -m "refactor(aop): 迁移 event_queue 到 pkg/aop/queue/"
```

---

## Task 3: 实现 Registry 核心分发器

**Files:**
- Create: `src/pkg/aop/core/registry.rs`

- [ ] **Step 1: 创建 src/pkg/aop/core/registry.rs**

```rust
//! Registry 事件分发器
//!
//! 核心职责：
//! - 管理生产者/消费者注册
//! - 事件分发：同步消费者直接调用，异步消费者入队
//! - 管理每个异步消费者的独立队列

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use common::error::{err, Result};
use crate::pkg::RequestContext;

use super::{Event, EventKind, Producer, Consumer, ConsumeMode};
use crate::pkg::aop::queue::{EventQueue, InMemoryEventQueue};

/// 事件注册中心
pub struct Registry {
    producers: RwLock<Vec<Arc<dyn Producer>>>,
    consumers: RwLock<HashMap<EventKind, Vec<Arc<dyn Consumer>>>>,
    queues: RwLock<HashMap<String, Arc<dyn EventQueue>>>,
    started: RwLock<bool>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            producers: RwLock::new(Vec::new()),
            consumers: RwLock::new(HashMap::new()),
            queues: RwLock::new(HashMap::new()),
            started: RwLock::new(false),
        }
    }

    // === 生产者管理 ===

    pub fn register_producer(&self, producer: Arc<dyn Producer>) -> Result<()> {
        let mut producers = self.producers.write()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        let name = producer.name();
        if producers.iter().any(|p| p.name() == name) {
            return Err(err!(Conflict, "producer already registered: {}", name));
        }

        producers.push(producer);
        Ok(())
    }

    // === 消费者管理 ===

    pub fn register_consumer(&self, consumer: Arc<dyn Consumer>) -> Result<()> {
        let name = consumer.name().to_string();

        // 异步消费者：创建独立队列
        if consumer.consume_mode() == ConsumeMode::Async {
            let queue: Arc<dyn EventQueue> = Arc::new(
                InMemoryEventQueue::<serde_json::Value>::new()
            );
            self.queues.write()
                .map_err(|e| err!(Internal, "registry lock error: {}", e))?
                .insert(name.clone(), queue);
        }

        // 按事件类型索引
        let mut consumers = self.consumers.write()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        for kind in consumer.interested_events() {
            consumers.entry(kind)
                .or_insert_with(Vec::new)
                .push(consumer.clone());
        }

        Ok(())
    }

    // === 事件发布 ===

    pub async fn publish<E: Event>(&self, event: E) {
        let kind = event.kind();

        let consumers = match self.consumers.read() {
            Ok(c) => c,
            Err(e) => {
                sys_error!("registry read error: {}", e);
                return;
            }
        };

        let Some(interested) = consumers.get(&kind) else {
            return;
        };

        for consumer in interested {
            match consumer.consume_mode() {
                ConsumeMode::Sync => {
                    if let Err(e) = consumer.on_event(&event).await {
                        sys_error!("consumer {} sync error: {}", consumer.name(), e);
                    }
                }
                ConsumeMode::Async => {
                    if let Ok(queues) = self.queues.read() {
                        if let Some(queue) = queues.get(consumer.name()) {
                            let ctx = RequestContext::new(None, None);
                            if let Err(e) = queue.enqueue(ctx, event.clone()).await {
                                sys_error!("consumer {} enqueue error: {}", consumer.name(), e);
                            }
                        }
                    }
                }
            }
        }
    }

    // === 异步消费 ===

    pub async fn dequeue_for<E: Event>(&self, consumer_name: &str) -> Result<Option<E>> {
        let queues = self.queues.read()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        let queue = queues.get(consumer_name)
            .ok_or_else(|| err!(NotFound, "consumer queue not found: {}", consumer_name))?;

        let ctx = RequestContext::new(None, None);
        queue.dequeue_next(ctx).await
    }

    pub async fn ack(&self, consumer_name: &str, event_id: &str) -> Result<()> {
        let queues = self.queues.read()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        let queue = queues.get(consumer_name)
            .ok_or_else(|| err!(NotFound, "consumer queue not found: {}", consumer_name))?;

        let ctx = RequestContext::new(None, None);
        queue.ack(ctx, event_id).await
    }

    pub async fn nack(&self, consumer_name: &str, event_id: &str) -> Result<()> {
        let queues = self.queues.read()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        let queue = queues.get(consumer_name)
            .ok_or_else(|| err!(NotFound, "consumer queue not found: {}", consumer_name))?;

        let ctx = RequestContext::new(None, None);
        queue.nack(ctx, event_id).await
    }

    // === 生命周期 ===

    pub async fn start_all(&self) -> Result<()> {
        let mut started = self.started.write()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;

        if *started {
            return Ok(());
        }

        let producers = self.producers.read()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?
            .clone();

        for producer in producers {
            if let Err(e) = producer.start().await {
                sys_warn!("producer {} start error: {}", producer.name(), e);
            }
        }

        *started = true;
        Ok(())
    }

    pub async fn stop_all(&self) -> Result<()> {
        let producers = self.producers.read()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?
            .clone();

        for producer in producers {
            if let Err(e) = producer.stop().await {
                sys_warn!("producer {} stop error: {}", producer.name(), e);
            }
        }

        let mut started = self.started.write()
            .map_err(|e| err!(Internal, "registry lock error: {}", e))?;
        *started = false;

        Ok(())
    }

    // === 查询 ===

    pub fn consumer_count(&self) -> usize {
        self.consumers.read()
            .map(|c| c.values().map(|v| v.len()).sum())
            .unwrap_or(0)
    }

    pub fn producer_count(&self) -> usize {
        self.producers.read()
            .map(|p| p.len())
            .unwrap_or(0)
    }

    pub fn queue_len(&self, consumer_name: &str) -> usize {
        self.queues.read()
            .ok()
            .and_then(|q| q.get(consumer_name))
            .map(|q| q.len())
            .unwrap_or(0)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: 运行 cargo check 验证**

Run: `cargo check`
Expected: 除了已删除的 event_queue 引用外，Registry 本身应编译通过

- [ ] **Step 3: 提交**

```bash
git add src/pkg/aop/core/registry.rs
git commit -m "feat(aop): 实现 Registry 事件分发器"
```

---

## Task 4: 实现消息系统事件 + AgentAwakeningConsumer

**Files:**
- Create: `src/pkg/aop/impl/mod.rs`
- Create: `src/pkg/aop/impl/message/mod.rs`
- Create: `src/pkg/aop/impl/message/events.rs`
- Create: `src/pkg/aop/impl/message/consumers.rs`

- [ ] **Step 1: 创建 src/pkg/aop/impl/mod.rs**

```rust
//! AOP 业务实现

pub mod message;
```

- [ ] **Step 2: 创建 src/pkg/aop/impl/message/mod.rs**

```rust
mod events;
mod consumers;

pub use events::MessageCreatedEvent;
pub use consumers::AgentAwakeningConsumer;
```

- [ ] **Step 3: 创建 src/pkg/aop/impl/message/events.rs**

```rust
//! 消息系统事件

use serde::{Serialize, Deserialize};
use crate::pkg::aop::{Event, EventKind};

/// 消息创建事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCreatedEvent {
    pub message_id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub from_id: String,
    pub from_role: i32,
    pub to_id: String,
    pub to_role: i32,
    pub message_type: i32,
    pub content: String,
    pub created_at: i64,
}

impl Event for MessageCreatedEvent {
    fn kind(&self) -> EventKind {
        EventKind::MESSAGE_CREATED
    }

    fn id(&self) -> &str {
        &self.message_id
    }

    fn order_key(&self) -> &str {
        self.project_id.as_deref().unwrap_or("")
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl MessageCreatedEvent {
    pub fn from_message(message: &crate::models::message::Message) -> Self {
        Self {
            message_id: message.id().to_string(),
            project_id: message.project_id().map(|s| s.to_string()),
            task_id: message.task_id().map(|s| s.to_string()),
            from_id: message.from_id().to_string(),
            from_role: message.from_role() as i32,
            to_id: message.to_id().to_string(),
            to_role: message.to_role() as i32,
            message_type: message.message_type() as i32,
            content: message.content().to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}
```

- [ ] **Step 4: 创建 src/pkg/aop/impl/message/consumers.rs**

```rust
//! 消息系统消费者

use async_trait::async_trait;
use common::error::Result;
use crate::pkg::aop::{Consumer, ConsumeMode, Event, EventKind};

/// Agent 唤醒消费者
///
/// 异步消费 MessageCreatedEvent，触发 Agent 唤醒。
/// 这是原来 consumer/message.rs 的核心逻辑，迁移到此处。
pub struct AgentAwakeningConsumer;

#[async_trait]
impl Consumer for AgentAwakeningConsumer {
    fn name(&self) -> &str {
        "agent.awakening"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::MESSAGE_CREATED]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    async fn on_event(&self, event: &dyn Event) -> Result<()> {
        use super::events::MessageCreatedEvent;

        // 反序列化获取具体类型
        let msg_event: MessageCreatedEvent = serde_json::from_str(
            &serde_json::to_string(event)?
        )?;

        sys_debug!("[AgentAwakeningConsumer] received: message_id={}", msg_event.message_id);

        // TODO: 调用 RuntimeDomain.wake_agent_brain()
        // 保持原有逻辑不变，后续从 consumer/message.rs 迁移
        // 暂时只记录日志，确保链路通畅

        Ok(())
    }
}
```

- [ ] **Step 5: 确保 src/pkg/aop/mod.rs 中有 pub mod impls**

检查并添加：
```rust
pub mod impl as impls;
```

- [ ] **Step 6: 运行 cargo check 验证**

Run: `cargo check`
Expected: 新代码编译通过

- [ ] **Step 7: 提交**

```bash
git add src/pkg/aop/impl/
git commit -m "feat(aop): 实现消息系统事件和 AgentAwakeningConsumer"
```

---

## Task 5: 集成测试（Registry 端到端）

**Files:**
- Create: `src/pkg/aop/core/registry_test.rs`

- [ ] **Step 1: 创建测试文件**

```rust
//! Registry 集成测试

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::pkg::aop::{Event, EventKind, Consumer, ConsumeMode, Registry};
use async_trait::async_trait;
use common::error::Result;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestEvent {
    id: String,
    value: i32,
}

impl Event for TestEvent {
    fn kind(&self) -> EventKind {
        EventKind::MESSAGE_CREATED
    }
    fn id(&self) -> &str { &self.id }
}

// === 同步消费者测试 ===

struct SyncCounter {
    name: String,
    count: AtomicUsize,
}

#[async_trait]
impl Consumer for SyncCounter {
    fn name(&self) -> &str { &self.name }
    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::MESSAGE_CREATED]
    }
    fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Sync }

    async fn on_event(&self, _event: &dyn Event) -> Result<()> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn test_sync_consumer_receives_event() {
    let registry = Registry::new();
    let consumer = Arc::new(SyncCounter {
        name: "sync.test".to_string(),
        count: AtomicUsize::new(0),
    });

    registry.register_consumer(consumer.clone()).unwrap();

    let event = TestEvent { id: "t1".to_string(), value: 42 };
    registry.publish(event).await;

    assert_eq!(consumer.count.load(Ordering::SeqCst), 1);
}

// === 异步消费者测试 ===

struct AsyncCounter {
    name: String,
}

#[async_trait]
impl Consumer for AsyncCounter {
    fn name(&self) -> &str { &self.name }
    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::MESSAGE_CREATED]
    }
    fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Async }

    async fn on_event(&self, _event: &dyn Event) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_async_consumer_enqueues() {
    let registry = Registry::new();
    let consumer = Arc::new(AsyncCounter {
        name: "async.test".to_string(),
    });

    registry.register_consumer(consumer).unwrap();

    let event = TestEvent { id: "t2".to_string(), value: 100 };
    registry.publish(event).await;

    // 从队列取出
    let dequeued: Option<TestEvent> = registry.dequeue_for("async.test").await.unwrap();
    assert!(dequeued.is_some());
    assert_eq!(dequeued.as_ref().unwrap().id, "t2");
    assert_eq!(dequeued.unwrap().value, 100);
}

#[tokio::test]
async fn test_async_consumer_ack() {
    let registry = Registry::new();
    let consumer = Arc::new(AsyncCounter {
        name: "async.ack".to_string(),
    });

    registry.register_consumer(consumer).unwrap();

    let event = TestEvent { id: "t3".to_string(), value: 1 };
    registry.publish(event).await;

    let dequeued: Option<TestEvent> = registry.dequeue_for("async.ack").await.unwrap();
    assert!(dequeued.is_some());
    let event = dequeued.unwrap();

    // ack 后队列空
    registry.ack("async.ack", event.id()).await.unwrap();
    assert_eq!(registry.queue_len("async.ack"), 0);
}

// === 多消费者测试 ===

#[tokio::test]
async fn test_multiple_consumers() {
    let registry = Registry::new();

    let sync1 = Arc::new(SyncCounter {
        name: "sync1".to_string(), count: AtomicUsize::new(0),
    });
    let sync2 = Arc::new(SyncCounter {
        name: "sync2".to_string(), count: AtomicUsize::new(0),
    });

    registry.register_consumer(sync1.clone()).unwrap();
    registry.register_consumer(sync2.clone()).unwrap();

    let event = TestEvent { id: "t4".to_string(), value: 0 };
    registry.publish(event).await;

    assert_eq!(sync1.count.load(Ordering::SeqCst), 1);
    assert_eq!(sync2.count.load(Ordering::SeqCst), 1);
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test --lib -- aop::core::registry_test`
Expected: 全部测试通过

- [ ] **Step 3: 提交**

```bash
git add src/pkg/aop/core/registry_test.rs
git commit -m "test(aop): 添加 Registry 集成测试"
```

---

## Task 6: 迁移 MessageDomain 发布事件 + 迁移引用点

**Files:**
- Modify: `src/service/domain/message/delivery.rs`
- Modify: `src/service/dal/message.rs`
- Modify: `src/consumer/scheduler.rs`
- Modify: `src/scheduler/mod.rs`
- Modify: `src/handlers/a2a/integration_test.rs`
- Modify: `src/handlers/a2a/send_task.rs`
- Modify: `src/service/domain/message/delivery_test.rs`
- Modify: `src/service/dal/message_test.rs`
- Modify: `src/service/domain/message/management_test.rs`

这是最关键的一步：把所有 `event_queue::dao().enqueue()` 替换为 `aop::publish()`，
把所有 `event_queue::dao().dequeue_next()` 替换为 `aop::registry().dequeue_for()`。

- [ ] **Step 1: 修改 delivery.rs - send_to_agent 改为发布事件**

找到 `send_to_agent` 中入队的代码：

```rust
// 原代码（示例，具体以实际为准）
let _ = crate::service::dao::event_queue::in_memory::message_dao()
    .enqueue(ctx.clone(), Box::new(message.clone()));
```

替换为：

```rust
use crate::pkg::aop;
use crate::pkg::aop::impls::message::MessageCreatedEvent;

let event = MessageCreatedEvent::from_message(&message);
aop::publish(event).await;
```

- [ ] **Step 2: 修改 delivery.rs - dequeue_next 方法**

如果 delivery 中有 `dequeue_next` 方法，改为从 registry 取：

```rust
// 原代码（示例）
pub async fn dequeue_next(&self, ctx: RequestContext) -> Result<Option<Message>> {
    crate::service::dao::event_queue::in_memory::message_dao()
        .dequeue_next(ctx)
}

// 新代码
pub async fn dequeue_next(&self, _ctx: RequestContext) -> Result<Option<Message>> {
    // TODO: 从 aop registry 取，然后转回 Message
    // 注意：这里暂时保留方法签名，内部改为走 registry
    // 后续 consumer 可以直接调用 aop::registry().dequeue_for()
    let event: Option<MessageCreatedEvent> =
        crate::pkg::aop::registry().dequeue_for("agent.awakening").await?;

    match event {
        Some(e) => {
            // TODO: 根据 event 重新构造 Message，或直接从 DB 加载
            // 短期方案：从 message_dal.get_by_id 加载
            // 这里需要根据实际情况调整
            Ok(None) // 占位，实际实现时填充
        }
        None => Ok(None),
    }
}
```

> 注意：delivery 层的 dequeue_next 是否保留，取决于 consumer 是否还通过 domain 层取消息。
> 如果 consumer 直接调用 aop::registry()，则 domain 层的 dequeue_next 可以删除。

- [ ] **Step 3: 修改 consumer/scheduler.rs - 消息消费者初始化**

原初始化调用 `init_message()`，改为注册消费者 + 消费方式调整。

- [ ] **Step 4: 修改 scheduler/mod.rs - CronTrigger 队列迁移**

CronTrigger 也走 AOP 框架，创建 `CronTriggerEvent` 和对应消费者。

- [ ] **Step 5: 修改 handlers/a2a/* - 集成测试初始化**

更新测试初始化代码，使用新的 AOP 初始化方式。

- [ ] **Step 6: 修改所有测试文件**

更新测试中的队列操作，改为事件发布/消费方式。

- [ ] **Step 7: 运行 cargo check 反复修复直到通过**

Run: `cargo check`
Expected: 编译通过，无错误

- [ ] **Step 8: 运行全部测试**

Run: `cargo test --lib`
Expected: 全部测试通过

- [ ] **Step 9: 提交**

```bash
git add src/service/ src/consumer/ src/scheduler/ src/handlers/
git commit -m "refactor: 迁移消息系统到 AOP 事件中心"
```

---

## Task 7: 文档更新 + 收尾

**Files:**
- Create: `docs/aop_event_center_design.md`
- Update: `docs/message_channel_design.md`

- [ ] **Step 1: 创建设计文档**

记录架构设计、核心概念、使用方式、迁移路径。

- [ ] **Step 2: 更新相关文档**

更新涉及 event_queue 的设计文档。

- [ ] **Step 3: 最终测试**

Run: `cargo test --lib && cargo check`
Expected: 全部通过

- [ ] **Step 4: 提交**

```bash
git add docs/
git commit -m "docs: 添加 AOP 事件中心设计文档"
```

---

## 验收标准

1. ✅ `service/dao/event_queue/` 目录已删除
2. ✅ `pkg/aop/` 包含完整框架（core + queue + impl）
3. ✅ MessageDomain.send_to_agent 通过 `aop::publish()` 发布事件
4. ✅ Consumer 通过 `aop::registry().dequeue_for()` 取事件
5. ✅ 所有测试通过（779+）
6. ✅ cargo check 无警告
7. ✅ 文档完整

---

## 后续可做事项（不在本计划内）

1. **迁移 consumer/message.rs 逻辑到 AgentAwakeningConsumer**
2. **实现 SsePushConsumer（同步消费者）**
3. **实现 A2aCallbackConsumer（异步消费者）**
4. **外部消息入站生产者（飞书等）**
5. **持久化队列实现（SQLite/Redis）**
6. **监控指标（队列长度、消费延迟、失败率）**
